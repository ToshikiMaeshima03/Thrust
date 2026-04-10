use thrust::{
    AgentMover, DirectionalLight, Editor, Entity, FogUniform, InstancedMesh, KeyCode, Material,
    NavMesh, NavMeshBuilder, PointLight, Resources, RigidBody, SpotLight, ThrustAppHandler,
    Transform, World, despawn, find_path, spawn_object, spawn_sphere,
};

struct PrimitivesDemo {
    cube_entity: Option<Entity>,
    sphere_entity: Option<Entity>,
    spawned_count: u32,
    /// Round 5: navmesh + AI agent
    navmesh: Option<NavMesh>,
    agent_entity: Option<Entity>,
    fog_enabled: bool,
    /// Round 9: ゲーム内エディタ
    editor: Editor,
}

impl ThrustAppHandler for PrimitivesDemo {
    fn init(&mut self, world: &mut World, res: &mut Resources) {
        // 床（灰色の PBR 平面）
        let plane = thrust::create_plane(&res.gpu.device, 10.0);
        spawn_object(
            world,
            plane,
            Transform {
                translation: glam::Vec3::new(0.0, -0.5, 0.0),
                ..Default::default()
            },
            Material::dielectric(glam::Vec3::splat(0.4), 0.8),
        );

        // 赤いキューブ (Round 4: 動的剛体)
        let cube = thrust::create_cube(&res.gpu.device, 1.0);
        let cube_entity = spawn_object(
            world,
            cube,
            Transform {
                translation: glam::Vec3::new(0.0, 2.0, 0.0),
                ..Default::default()
            },
            Material::dielectric(glam::Vec3::new(1.0, 0.2, 0.2), 0.5),
        );
        // 物理: 重力で落ちる動的剛体
        let _ = world.insert(
            cube_entity,
            (
                RigidBody::dynamic(),
                thrust::Collider {
                    shape: thrust::ColliderShape::Aabb(thrust::Aabb::new(
                        glam::Vec3::splat(-0.5),
                        glam::Vec3::splat(0.5),
                    )),
                    is_trigger: false,
                },
            ),
        );
        self.cube_entity = Some(cube_entity);

        // 金属球 (Round 4: PBR メタリック)
        let sphere = thrust::create_sphere(&res.gpu.device, 0.5, 32, 16);
        self.sphere_entity = Some(spawn_object(
            world,
            sphere,
            Transform {
                translation: glam::Vec3::new(2.0, 0.0, 0.0),
                ..Default::default()
            },
            Material::metallic(glam::Vec3::new(0.95, 0.93, 0.88), 0.2),
        ));

        // PBR スフィアグリッド (5x5、metallic 0..1 × roughness 0..1)
        for j in 0..5 {
            for i in 0..5 {
                let metallic = i as f32 / 4.0;
                let roughness = (j as f32 / 4.0).clamp(0.05, 1.0);
                let mut mat = Material::dielectric(glam::Vec3::new(0.95, 0.65, 0.30), roughness);
                mat.metallic_factor = metallic;
                let s = thrust::create_sphere(&res.gpu.device, 0.35, 24, 12);
                spawn_object(
                    world,
                    s,
                    Transform {
                        translation: glam::Vec3::new(
                            -3.0 + i as f32 * 0.9,
                            1.5 + j as f32 * 0.9,
                            -3.0,
                        ),
                        ..Default::default()
                    },
                    mat,
                );
            }
        }

        // Round 4: 複数光源
        // 追加 directional light (青色、上から)
        world.spawn((DirectionalLight {
            direction: glam::Vec3::new(-0.3, -1.0, -0.2).normalize(),
            color: glam::Vec3::new(0.4, 0.6, 1.0),
            intensity: 0.5,
        },));

        // 点光源 3 つ
        world.spawn((
            Transform::from_translation(glam::Vec3::new(-2.0, 1.5, 2.0)),
            PointLight {
                color: glam::Vec3::new(1.0, 0.3, 0.3),
                intensity: 5.0,
                range: 6.0,
            },
        ));
        world.spawn((
            Transform::from_translation(glam::Vec3::new(2.0, 1.5, 2.0)),
            PointLight {
                color: glam::Vec3::new(0.3, 1.0, 0.3),
                intensity: 5.0,
                range: 6.0,
            },
        ));
        world.spawn((
            Transform::from_translation(glam::Vec3::new(0.0, 3.0, -2.0)),
            PointLight {
                color: glam::Vec3::new(0.3, 0.3, 1.0),
                intensity: 5.0,
                range: 8.0,
            },
        ));

        // スポット光源 1 つ
        world.spawn((
            Transform::from_translation(glam::Vec3::new(0.0, 5.0, 0.0)),
            SpotLight {
                color: glam::Vec3::ONE,
                intensity: 8.0,
                range: 15.0,
                inner_angle: 0.3,
                outer_angle: 0.6,
                direction: glam::Vec3::new(0.0, -1.0, 0.0),
            },
        ));

        // Round 5: GPU インスタンシング (foliage 風の小さなキューブを 100 個)
        let foliage_mesh = thrust::create_cube(&res.gpu.device, 0.2);
        let mut instances = Vec::with_capacity(100);
        for i in 0..10 {
            for j in 0..10 {
                let x = -4.5 + i as f32;
                let z = -4.5 + j as f32;
                instances.push(Transform {
                    translation: glam::Vec3::new(x, -0.4, z),
                    rotation: glam::Quat::from_rotation_y((i as f32) * 0.5 + (j as f32) * 0.3),
                    scale: glam::Vec3::splat(1.0),
                });
            }
        }
        let foliage = InstancedMesh::new(
            &res.gpu.device,
            foliage_mesh,
            instances,
            Material::dielectric(glam::Vec3::new(0.3, 0.6, 0.2), 0.7),
        );
        world.spawn((foliage,));

        // Round 5: ボリュメトリックフォグ (デフォルト無効、F キーで切替)
        self.fog_enabled = false;

        // Round 5: navmesh 構築 + AI エージェント
        let mut nm_builder = NavMeshBuilder::new(glam::Vec3::new(-5.0, 0.0, -5.0), 0.5, 20, 20);
        // キューブ周辺を障害物としてマーク
        nm_builder.add_circle_obstacle(glam::Vec3::new(0.0, 0.0, 0.0), 1.0);
        let navmesh = nm_builder.build();
        self.navmesh = Some(navmesh);

        // エージェント (オレンジ球が動き回る)
        let agent_mesh = thrust::create_sphere(&res.gpu.device, 0.2, 16, 8);
        let agent_entity = spawn_object(
            world,
            agent_mesh,
            Transform::from_translation(glam::Vec3::new(-4.0, 0.0, -4.0)),
            Material::dielectric(glam::Vec3::new(1.0, 0.5, 0.0), 0.3),
        );
        let _ = world.insert_one(agent_entity, AgentMover::new(2.0));
        self.agent_entity = Some(agent_entity);

        log::info!(
            "プリミティブデモ初期化完了 (Round 5: CSM + フォグ + インスタンシング + navmesh)"
        );
        log::info!(
            "操作: Space=球追加, Delete=球削除, 矢印キー=ライト方向, F=フォグ, P=エージェント新パス"
        );
    }

    fn update(&mut self, world: &mut World, res: &mut Resources, dt: f32) {
        // Round 5: AI エージェント移動
        thrust::agent_movement_system(world, dt);

        // Round 5: F キーでフォグ切替
        if res.input.is_key_pressed(KeyCode::KeyF) {
            self.fog_enabled = !self.fog_enabled;
            let fog = if self.fog_enabled {
                FogUniform::outdoor(glam::Vec3::new(0.7, 0.8, 0.9), 0.04)
            } else {
                FogUniform::default()
            };
            res.fog.update(&res.gpu.queue, fog);
            log::info!("フォグ: {}", if self.fog_enabled { "ON" } else { "OFF" });
        }

        // Round 5: P キーでエージェントに新パス計算
        if res.input.is_key_pressed(KeyCode::KeyP)
            && let (Some(navmesh), Some(agent_e)) = (&self.navmesh, self.agent_entity)
        {
            let current_pos = world
                .get::<&Transform>(agent_e)
                .map(|t| t.translation)
                .unwrap_or(glam::Vec3::ZERO);
            // ランダムっぽい目標
            let target_x = ((self.spawned_count as f32 * 1.7) % 8.0) - 4.0;
            let target_z = ((self.spawned_count as f32 * 2.3) % 8.0) - 4.0;
            let target = glam::Vec3::new(target_x, 0.0, target_z);
            let path = find_path(navmesh, current_pos, target);
            if let Ok(mut agent) = world.get::<&mut AgentMover>(agent_e) {
                agent.set_path(path);
                log::info!(
                    "エージェント新パス: {} ウェイポイント, target=({:.1}, {:.1})",
                    agent.path.len(),
                    target_x,
                    target_z
                );
            }
        }

        // Space キーで新しい球体を追加 (PBR + 物理)
        if res.input.is_key_pressed(KeyCode::Space) {
            self.spawned_count += 1;
            let t = Transform {
                translation: glam::Vec3::new(
                    -2.0 + (self.spawned_count % 5) as f32,
                    5.0,
                    -2.0 + (self.spawned_count / 5) as f32,
                ),
                ..Default::default()
            };

            // ランダム風の色
            let r = ((self.spawned_count * 73) % 256) as f32 / 255.0;
            let g = ((self.spawned_count * 137) % 256) as f32 / 255.0;
            let b = ((self.spawned_count * 199) % 256) as f32 / 255.0;

            let entity = spawn_sphere(
                world,
                res,
                0.3,
                16,
                8,
                t,
                Material::dielectric(glam::Vec3::new(r, g, b), 0.4),
            );
            // 物理: 落下する球
            let _ = world.insert(
                entity,
                (
                    RigidBody::dynamic(),
                    thrust::Collider {
                        shape: thrust::ColliderShape::Sphere {
                            center: glam::Vec3::ZERO,
                            radius: 0.3,
                        },
                        is_trigger: false,
                    },
                ),
            );
            log::info!("球体追加 (合計: {})", self.spawned_count);
        }

        // Delete キーで球体を削除
        if (res.input.is_key_pressed(KeyCode::Delete)
            || res.input.is_key_pressed(KeyCode::Backspace))
            && let Some(entity) = self.sphere_entity.take()
        {
            despawn(world, entity);
            log::info!("球体を削除しました");
        }

        // 矢印キーでライト方向を変更 (1 つ目の DirectionalLight に影響)
        let light_speed = 2.0 * dt;
        if let Some(light) = world
            .query_mut::<&mut DirectionalLight>()
            .into_iter()
            .next()
        {
            if res.input.is_key_held(KeyCode::ArrowLeft) {
                light.direction = glam::Quat::from_rotation_y(light_speed) * light.direction;
            }
            if res.input.is_key_held(KeyCode::ArrowRight) {
                light.direction = glam::Quat::from_rotation_y(-light_speed) * light.direction;
            }
            if res.input.is_key_held(KeyCode::ArrowUp) {
                light.direction = glam::Quat::from_rotation_x(-light_speed) * light.direction;
            }
            if res.input.is_key_held(KeyCode::ArrowDown) {
                light.direction = glam::Quat::from_rotation_x(light_speed) * light.direction;
            }
        }
    }

    /// Round 9: フル機能エディタを表示
    fn ui(&mut self, ctx: &thrust::egui::Context, world: &mut World, res: &mut Resources) {
        // メインのエディタ (アウトライナ + インスペクタ + Spawn メニュー + Render 設定 + パフォーマンス)
        self.editor.show(ctx, world, res);

        // 操作ヒント (右下)
        thrust::egui::Window::new("操作ヒント")
            .default_pos([10.0, 600.0])
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.label("F: フォグ ON/OFF");
                ui.label("P: AI エージェント新パス");
                ui.label("Space: 球追加 (物理付き)");
                ui.label("Delete: 球削除");
                ui.label("矢印: ライト方向");
                ui.label("マウスドラッグ: カメラ回転");
                ui.label("ホイール: ズーム");
                ui.separator();
                ui.label(format!("生成カウント: {}", self.spawned_count));
            });
    }
}

fn main() {
    env_logger::init();
    log::info!("Thrust Primitives Demo 起動 (Round 9 - エディタ統合)");
    log::info!("==================================================");
    log::info!("ゲーム内エディタ機能:");
    log::info!("  - 左パネル: アウトライナ (エンティティ一覧/検索/削除/複製)");
    log::info!("  - 右パネル: インスペクタ (Transform/Material/Light 編集)");
    log::info!("  - 左中央: Spawn メニュー (Cube/Sphere/Plane/各種ライト)");
    log::info!("  - 左下: レンダリング設定 (Fog/SSAO/SSR/DOF/Color Grading)");
    log::info!("  - 右上: パフォーマンス HUD (FPS/Entities/Lights)");
    log::info!("  - メニュー: シーン Save/Load + ツール切替");
    log::info!("==================================================");

    if let Err(e) = thrust::run(PrimitivesDemo {
        cube_entity: None,
        sphere_entity: None,
        spawned_count: 0,
        navmesh: None,
        agent_entity: None,
        fog_enabled: false,
        editor: Editor::new(),
    }) {
        log::error!("エンジン起動失敗: {e}");
        std::process::exit(1);
    }
}

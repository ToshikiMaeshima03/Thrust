use forge3d::{
    ActiveDirectionalLight, DirectionalLight, Entity, ForgeAppHandler, KeyCode, Material,
    Resources, Transform, World, despawn, spawn_object, spawn_sphere,
};

struct PrimitivesDemo {
    cube_entity: Option<Entity>,
    sphere_entity: Option<Entity>,
    spawned_count: u32,
}

impl ForgeAppHandler for PrimitivesDemo {
    fn init(&mut self, world: &mut World, res: &mut Resources) {
        // 床（灰色の平面）
        let plane = forge3d::create_plane(&res.gpu.device, 10.0);
        spawn_object(
            world,
            plane,
            Transform {
                translation: glam::Vec3::new(0.0, -0.5, 0.0),
                ..Default::default()
            },
            Material {
                base_color: glam::Vec4::new(0.4, 0.4, 0.4, 1.0),
                texture: None,
            },
        );

        // 赤いキューブ
        let cube = forge3d::create_cube(&res.gpu.device, 1.0);
        self.cube_entity = Some(spawn_object(
            world,
            cube,
            Transform::default(),
            Material {
                base_color: glam::Vec4::new(1.0, 0.2, 0.2, 1.0),
                texture: None,
            },
        ));

        // 青い球体
        let sphere = forge3d::create_sphere(&res.gpu.device, 0.5, 32, 16);
        self.sphere_entity = Some(spawn_object(
            world,
            sphere,
            Transform {
                translation: glam::Vec3::new(2.0, 0.0, 0.0),
                ..Default::default()
            },
            Material {
                base_color: glam::Vec4::new(0.2, 0.4, 1.0, 1.0),
                texture: None,
            },
        ));

        log::info!("プリミティブデモ初期化完了");
        log::info!("操作: Space=球追加, Delete=球削除, 矢印キー=ライト方向変更");
    }

    fn update(&mut self, world: &mut World, res: &mut Resources, dt: f32) {
        // キューブを回転
        if let Some(entity) = self.cube_entity
            && let Ok(mut t) = world.get::<&mut Transform>(entity)
        {
            t.rotation *= glam::Quat::from_rotation_y(dt);
        }

        // Space キーで新しい球体を追加
        if res.input.is_key_pressed(KeyCode::Space) {
            self.spawned_count += 1;
            let t = Transform {
                translation: glam::Vec3::new(
                    -2.0 + (self.spawned_count % 5) as f32,
                    0.0,
                    -2.0 + (self.spawned_count / 5) as f32,
                ),
                ..Default::default()
            };

            // ランダム風の色
            let r = ((self.spawned_count * 73) % 256) as f32 / 255.0;
            let g = ((self.spawned_count * 137) % 256) as f32 / 255.0;
            let b = ((self.spawned_count * 199) % 256) as f32 / 255.0;

            spawn_sphere(
                world,
                res,
                0.3,
                16,
                8,
                t,
                Material {
                    base_color: glam::Vec4::new(r, g, b, 1.0),
                    texture: None,
                },
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

        // 矢印キーでライト方向を変更
        let light_speed = 2.0 * dt;
        for (light, _) in world.query_mut::<(&mut DirectionalLight, &ActiveDirectionalLight)>() {
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
}

fn main() {
    env_logger::init();
    log::info!("forge3d Primitives Demo 起動");

    forge3d::run(PrimitivesDemo {
        cube_entity: None,
        sphere_entity: None,
        spawned_count: 0,
    });
}

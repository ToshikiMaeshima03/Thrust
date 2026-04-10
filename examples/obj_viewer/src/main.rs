use thrust::{Resources, ThrustAppHandler, Transform, World};

struct ModelViewerApp {
    model_path: String,
}

impl ThrustAppHandler for ModelViewerApp {
    fn init(&mut self, world: &mut World, res: &mut Resources) {
        log::info!("モデル読み込み: {}", self.model_path);

        let entities = thrust::spawn_model(world, res, &self.model_path, Transform::default())
            .expect("モデルの読み込みに失敗しました");

        log::info!("{}個のメッシュエンティティを生成しました", entities.len());
    }
}

fn main() {
    env_logger::init();

    let model_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assets/models/cube.obj".to_string());

    log::info!("Thrust Model Viewer 起動");

    thrust::run(ModelViewerApp { model_path }).expect("エンジン起動失敗");
}

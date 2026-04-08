use std::path::Path;

use forge3d::{ForgeAppHandler, Material, Resources, Transform, World, spawn_object};

struct ObjViewerApp {
    obj_path: String,
}

impl ForgeAppHandler for ObjViewerApp {
    fn init(&mut self, world: &mut World, res: &mut Resources) {
        let path = Path::new(&self.obj_path);
        log::info!("OBJ読み込み: {}", path.display());

        let meshes =
            forge3d::load_obj(&res.gpu.device, path).expect("OBJファイルの読み込みに失敗しました");

        log::info!("{}個のメッシュを読み込みました", meshes.len());

        for mesh in meshes {
            spawn_object(world, mesh, Transform::default(), Material::default());
        }
    }
}

fn main() {
    env_logger::init();

    let obj_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "assets/models/cube.obj".to_string());

    log::info!("forge3d OBJ Viewer 起動");

    forge3d::run(ObjViewerApp { obj_path });
}

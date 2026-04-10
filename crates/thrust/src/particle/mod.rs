mod emitter;
mod render;

pub use emitter::{Particle, ParticleEmitter, particle_system};
pub use render::{
    ParticleBatch, ParticleRenderState, create_particle_pipeline,
    create_particle_texture_bind_group_layout, create_particle_textured_pipeline,
    particle_render_prep_system, particle_render_system,
};

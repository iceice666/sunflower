
mod player;
mod provider;
mod source;
mod task_mgr;

pub fn init_logger() {
    let _ =tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .try_init();
}
pub fn init_logger() {
    if let Err(err) = simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init() {
        println!("Failed to initialize logger: {}", err);
    }
}
use poison_swamp_level::{Config, Garbage};

fn main() {
    let config = Config::read_from_file("./config.json").unwrap();

    let garbage = Garbage::new(&config);

    let output = garbage.render("/some/path");
    println!("{output}");
}

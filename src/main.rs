use poison_swamp_level::{Config, Garbage};

fn main() {
    let config = Config::default();

    let garbage = Garbage::new(&config);

    let output = garbage.render("/some/path");
    println!("{output}");
}

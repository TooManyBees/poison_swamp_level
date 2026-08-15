use poison_swamp_level::{Config, Garbage};

fn main() {
    let mut config = Config::default();
    config
        .garbage
        .source_files
        .push("./susan.sontag.notes.on.camp.txt".into());
    config.garbage.words_file = Some("./words.txt".into());
    config.garbage.poisons.push("perfidious".into());
    config.garbage.template_file = Some("./garbage.html".into());

    let garbage = Garbage::new(&config);

    let output = garbage.render("/some/path");
    println!("{output}");
}

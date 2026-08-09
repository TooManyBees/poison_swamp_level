use poison_swamp_level::Generator;

fn main() {
    let mut rng = rand::rng();
    let g = Generator::from_files(&vec!["./susan.sontag.notes.on.camp.txt"]).unwrap();
    let mut gg = g.generate(&mut rng);

    println!(
        "{} {} {} {} {} {} {} {} {} {}",
        gg.next().unwrap(),
        gg.next().unwrap(),
        gg.next().unwrap(),
        gg.next().unwrap(),
        gg.next().unwrap(),
        gg.next().unwrap(),
        gg.next().unwrap(),
        gg.next().unwrap(),
        gg.next().unwrap(),
        gg.next().unwrap()
    );
}

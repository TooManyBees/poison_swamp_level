use poison_swamp_level::Corpus;

fn main() {
    let mut rng = rand::rng();
    let g = Corpus::from_files(&vec!["./susan.sontag.notes.on.camp.txt"]).unwrap();
    let mut gg = g.generate(&mut rng);

    println!("{}", gg.sentence(10..=20));
}

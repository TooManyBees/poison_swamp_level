use poison_swamp_level::Corpus;

fn main() {
    let mut rng = rand::rng();
    let corpus = Corpus::from_files(&vec!["./susan.sontag.notes.on.camp.txt"]).unwrap();
    let mut generator = corpus.generator(&mut rng);

    for _ in 0..5 {
        println!("{}\n", generator.generate(10..=20));
    }
}

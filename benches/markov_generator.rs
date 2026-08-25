use criterion::{Criterion, criterion_group, criterion_main};
use poison_swamp_level::Corpus;
use rand_seeder::{Seeder, SipRng};
use std::hint::black_box;

fn criterion_benchmark(c: &mut Criterion) {
    let corpus = Corpus::from_files(&["susan.sontag.on.style.txt"]).unwrap();

    c.bench_function("list-based generator", |b| {
        let mut rng: SipRng = Seeder::from("/some/predictable/path").into_rng();
        b.iter(|| {
            let generator = corpus.generator(black_box(&mut rng));
            let words = generator.take(500);
            for word in words {
                black_box(word);
            }
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

pub mod color;
pub mod time;
pub mod wit_highlighter;
pub mod wit_type_formatter;

use std::rc::Rc;

pub fn trace_id() -> Rc<str> {
    use rand::SeedableRng as _;
    let mut rng = rand::rngs::SmallRng::from_os_rng();
    Rc::from(
        (0..5)
            .map(|_| rand::Rng::random_range(&mut rng, b'a'..=b'z') as char)
            .collect::<String>(),
    )
}

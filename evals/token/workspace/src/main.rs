//! Example binary for the token bench fixture.

use bench_fixture::{Canvas, Widget};

fn main() {
    let mut canvas = Canvas::new();
    canvas.add(Widget::new(1, "button"));
    canvas.add(Widget::new(2, "label"));
    println!("visible: {}", canvas.visible_count());
}

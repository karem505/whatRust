use tao::{event::{Event, WindowEvent}, event_loop::{ControlFlow, EventLoop}, window::WindowBuilder};
use wry::WebViewBuilder;
fn main() {
    let url = std::env::args().nth(1).unwrap();
    let child = std::env::args().nth(2).as_deref() == Some("child");
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().with_title("wryctl").build(&event_loop).unwrap();
    let b = WebViewBuilder::new().with_url(&url);
    let _wv = if child {
        b.with_bounds(wry::Rect { position: wry::dpi::LogicalPosition::new(0, 0).into(), size: wry::dpi::LogicalSize::new(900, 600).into() }).build_as_child(&window).unwrap()
    } else { b.build(&window).unwrap() };
    event_loop.run(move |event, _, cf| {
        *cf = ControlFlow::Wait;
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event { *cf = ControlFlow::Exit; }
    });
}

use crate::cpu::bus::Bus;
use crate::cpu::cpu::Cpu;
use crate::ppu::ppu::Ppu;
use crate::render;
use crate::render::frame::Frame;
use crate::rom::rom::Rom;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;

use sdl2::render::Canvas;
use sdl2::render::Texture;
use sdl2::video::Window;
use sdl2::EventPump;

pub fn run<'a>(
    rom: Rom,
    mut canvas: Canvas<Window>,
    mut event_pump: EventPump,
    mut texture: Texture<'a>,
    mut frame: Frame,
) {
    //BusとLoop処理の実装
    let bus = Bus::new(rom, move |ppu: &Ppu| {
        render::render(ppu, &mut frame);
        texture.update(None, &frame.data, 256 * 3).unwrap();

        //画面を描画
        canvas.copy(&texture, None, None).unwrap();
        //画面を更新
        canvas.present();

        //イベント処理
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => std::process::exit(0),
                _ => {}
            }
        }
    });

    //CPUエミュレート
    let mut cpu = Cpu::new(bus);
    cpu.reset();
    cpu.run();
}

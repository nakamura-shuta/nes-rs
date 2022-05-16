#[macro_use]
extern crate arrayref;
#[macro_use]
extern crate bitflags;

mod cpu;
mod nes;
mod ppu;
mod render;
mod rom;

use cpu::bus::Bus;
use cpu::cpu::Memory;
use sdl2::pixels::PixelFormatEnum;
use std::env;

use render::frame::Frame;
use rom::rom::Rom;

fn main() {
    //SDL初期化
    let sdl_context = sdl2::init().unwrap();
    // Videoサブシステム取得
    let video_subsystem = sdl_context.video().unwrap();
    //Wdnow作成
    let window = video_subsystem
        .window("NES Example", 500, 400)
        .position_centered()
        .build()
        .unwrap();
    //Canvasの作成
    let mut canvas = window.into_canvas().present_vsync().build().unwrap();
    canvas.set_scale(3.0, 3.0).unwrap();

    //ゲームのループ
    let event_pump = sdl_context.event_pump().unwrap();

    //Texture作成
    let creator = canvas.texture_creator();
    let texture = creator
        .create_texture_target(PixelFormatEnum::RGB24, 256, 240)
        .unwrap();

    //Frame作成
    let frame = Frame::new();

    //ROM読み出し
    let args: Vec<String> = env::args().collect();
    let nes_file = &args[1];
    let rom = Rom::load(nes_file).unwrap();

    //NESの実行
    nes::run(rom, canvas, event_pump, texture, frame);
}

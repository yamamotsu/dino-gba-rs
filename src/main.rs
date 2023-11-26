// Games made using `agb` are no_std which means you don't have access to the standard
// rust library. This is because the game boy advance doesn't really have an operating
// system, so most of the content of the standard library doesn't apply.
//
// Provided you haven't disabled it, agb does provide an allocator, so it is possible
// to use both the `core` and the `alloc` built in crates.
#![no_std]
// `agb` defines its own `main` function, so you must declare your game's main function
// using the #[agb::entry] proc macro. Failing to do so will cause failure in linking
// which won't be a particularly clear error message.
#![no_main]
// This is required to allow writing tests
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]
#![cfg_attr(test, test_runner(agb::test_runner::test_runner))]

extern crate alloc;

use agb::{
    display::{
        object::{Graphics, Tag},
        tile_data::TileData,
        tiled::{InfiniteScrolledMap, RegularBackgroundSize, TileFormat, TileSet, TileSetting},
        Priority,
    },
    fixnum::{num, Num, Vector2D},
    include_aseprite,
    input::Button,
    mgba::{DebugLevel, Mgba, self},
    rng,
};
use alloc::{boxed::Box, vec::Vec};

// Load background tiles as `bg_tiles` module
agb::include_background_gfx!(bg_tiles, tiles => "assets/gfx/dino_background.bmp");
const TILE_MAP_CSV_STR: &str = include_str!("../assets/tilemap/dino_map.csv");
const BG_TILE_DATA: TileData = bg_tiles::tiles;
const BG_TILE: TileSet = BG_TILE_DATA.tiles;
const BG_TILE_CONFIG: &[TileSetting] = BG_TILE_DATA.tile_settings;

const SPRITES: &Graphics = include_aseprite!("assets/gfx/dino.aseprite");

// We define some easy ways of referencing the sprites
const DINO: &Tag = SPRITES.tags().get("Dino");
const BIRD: &Tag = SPRITES.tags().get("Bird");
const SPRITE_ANIMATION_DELAY_FRAMES: u32 = 10;

const MAX_JUMP_HEIGHT_PX: u16 = 60;
const MAX_JUMP_DURATION_FRAMES: u16 = 18;

const GROUND_TILE_Y: u16 = 17;
const GROUND_Y: u16 = GROUND_TILE_Y * 8;
const DINO_GROUNDED_Y: u16 = GROUND_Y - 34;
const BIRD_SPAWN_INTERVAL_FRAMES: u16 = 60 * 5;
const LEVEL_UP_INTERVAL_FRAMES: u16 = 60 * 30;

fn frame_ranger(count: u32, start: u32, end: u32, delay: u32) -> usize {
    (((count / delay) % (end + 1 - start)) + start) as usize
}
fn print_info(mgba: &mut Mgba, output: core::fmt::Arguments) {
    // Debug output
    mgba.print(output, DebugLevel::Info).unwrap();
}

// The main function must take 1 arguments and never return. The agb::entry decorator
// ensures that everything is in order. `agb` will call this after setting up the stack
// and interrupt handlers correctly. It will also handle creating the `Gba` struct for you.
#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    let mut input = agb::input::ButtonController::new();
    let mut mgba: agb::mgba::Mgba = agb::mgba::Mgba::new().unwrap();
    let vblank = agb::interrupt::VBlank::get();

    // Debug output
    print_info(
        &mut mgba,
        format_args!("Tile format: {:?}", BG_TILE.format()),
    );
    print_info(&mut mgba, format_args!("Tile config: {:?}", BG_TILE_CONFIG));
    for color_idx in 0..16 {
        print_info(
            &mut mgba,
            format_args!(
                "PALETTE {color_idx:02}: {:b}",
                bg_tiles::PALETTES[0].colour(color_idx)
            ),
        );
    };

    // Physics
    let gravity_px_per_square_frame: Num<i32, 8> =
        Num::new(2 * MAX_JUMP_HEIGHT_PX as i32) / Num::new(MAX_JUMP_DURATION_FRAMES.pow(2) as i32);
    print_info(
        &mut mgba,
        format_args!("Gravity: {:?} px/frame^2", gravity_px_per_square_frame),
    );

    // Background Initialization
    //  1. Get Tiled mode graphics manager
    let (gfx, mut vram) = gba.display.video.tiled0();
    //  2. load TileMap (tile indices of each grid position)
    let tile_map: Vec<usize> = TILE_MAP_CSV_STR
        .splitn(64 * 32, [',', '\r', '\n'])
        .map(|s| usize::from_str_radix(s, 10).unwrap_or(0))
        .collect();
    //  3. set bg palettes
    vram.set_background_palettes(bg_tiles::PALETTES);
    //  4. create infinite scrolled background
    let mut background = InfiniteScrolledMap::new(
        gfx.background(
            Priority::P0,
            RegularBackgroundSize::Background64x32,
            TileFormat::FourBpp,
        ),
        Box::new(|pos| {
            let x = pos.x.rem_euclid(64);
            let y = pos.y.rem_euclid(20);
            let mut _mgba: agb::mgba::Mgba = agb::mgba::Mgba::new().unwrap();
            let tile_idx = *tile_map.get((x + 64 * y) as usize).unwrap_or(&0) as usize;
            let tile_config = BG_TILE_CONFIG[tile_idx];
            (&BG_TILE_DATA.tiles, tile_config)
        }),
    );
    background.init(&mut vram, (0, 0).into(), &mut || {});

    background.set_pos(&mut vram, (0, 0).into());
    background.commit(&mut vram);
    background.show();

    // Objects
    let object = gba.display.object.get_managed();
    let mut dino = object.object_sprite(DINO.sprite(0));
    let mut bird = object.object_sprite(BIRD.sprite(0));

    let mut frame_count: u32 = 0;
    let mut position: Vector2D<Num<i32, 8>> = (0, 0).into();
    let mut scroll_velocity: Num<i32, 8> = num!(2.0);
    let mut speed_level: u32 = 0;

    let mut bird_shown: bool = false;
    let mut bird_count: u16 = 0;
    let mut bird_position: Vector2D<Num<i32, 8>> = (0, 0).into();
    let bird_velocity: Num<i32, 8> = num!(-0.2);

    let mut dino_y: u16 = DINO_GROUNDED_Y;
    let mut dino_velocity_y: Num<i32, 8> = Num::new(0);
    let mut dino_grounded: bool = false;

    dino.set_x(16).set_y(dino_y).show();
    object.commit();

    loop {
        dino.set_sprite(object.sprite(DINO.sprite(frame_ranger(
            frame_count,
            0,
            1,
            SPRITE_ANIMATION_DELAY_FRAMES,
        ))));

        if dino_grounded {
            if input.is_just_pressed(Button::A) {
                dino_velocity_y = -gravity_px_per_square_frame * (MAX_JUMP_DURATION_FRAMES as i32);
                // print_info(&mut mgba, format_args!("jump up velocity: {:?}", dino_velocity_y));
                dino_grounded = false;
            }
        } else {
            // print_info(&mut mgba, format_args!("jumping velocity: {:?}", dino_velocity_y.floor()));
            dino_y = (dino_y as i32 + dino_velocity_y.floor()) as u16;
            if dino_y >= DINO_GROUNDED_Y {
                dino_y = DINO_GROUNDED_Y;
                dino_grounded = true;
            }
            dino.set_y(dino_y);
            dino_velocity_y += gravity_px_per_square_frame;
        }

        position.x += scroll_velocity;
        background.set_pos(&mut vram, position.floor());

        if frame_count >= ((speed_level + 1) * LEVEL_UP_INTERVAL_FRAMES as u32) {
            speed_level += 1;
            scroll_velocity += num!(0.1);
            print_info(&mut mgba, format_args!("lvl up -> {}, V={:2}", speed_level, scroll_velocity));
        }

        if bird_shown {
            bird.set_sprite(object.sprite(BIRD.sprite(frame_ranger(
                frame_count,
                0,
                1,
                SPRITE_ANIMATION_DELAY_FRAMES,
            ))));

            bird_position.x += bird_velocity - scroll_velocity;
            bird.set_position(bird_position.floor());

            if bird_position.x < Num::new(-32) {
                bird.hide();
                bird_shown = false;
            }
        } else if frame_count > (bird_count * BIRD_SPAWN_INTERVAL_FRAMES) as u32 {
            bird_count += 1;
            // Spawn bird
            let spawn_y: i32 = ((rng::gen() & 0b0111) + 5) * 8;
            bird_position.y = Num::new(spawn_y);
            bird_position.x = Num::new(8 * 30);
            bird_shown = true;
            bird.set_position(bird_position.floor());
            bird.show();
        }

        // Wait for vblank, then commit the objects to the screen
        vblank.wait_for_vblank();
        input.update();
        object.commit();
        background.commit(&mut vram);
        frame_count += 1;
    }
}

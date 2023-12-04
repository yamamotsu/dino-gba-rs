use core::ops::Range;

use agb::{
    display::{
        object::{OamIterator, ObjectUnmanaged, SpriteLoader, SpriteVram, Tag},
        tiled::{InfiniteScrolledMap, VRamManager},
    },
    fixnum::{num, Num, Rect, Vector2D},
    hash_map::HashMap,
    input::{Button, ButtonController},
    mgba::Mgba,
    sound::mixer::Mixer,
};
use alloc::{boxed::Box, collections::VecDeque, vec::Vec};

pub type Number = Num<i32, 8>;

pub mod resource {
    use agb::{
        display::{
            object::{Graphics, Sprite, Tag},
            palette16::Palette16,
            tile_data::TileData,
        },
        fixnum::{Rect, Vector2D},
        hash_map::HashMap,
        sound::mixer::SoundChannel,
    };
    use alloc::vec::Vec;

    const SPRITES: &Graphics = agb::include_aseprite!("assets/gfx/dino.aseprite");
    pub(super) const DINO: &Tag = SPRITES.tags().get("Dino");
    pub(super) const BIRD: &Tag = SPRITES.tags().get("Bird");
    pub(super) const CACTUS: &Sprite = SPRITES.tags().get("Cactus").sprite(0);

    const FONT_SPRITES: &Graphics = agb::include_aseprite!("assets/gfx/font.aseprite");
    pub(super) const CHAR_SPRITE_KEYS: [&'static str; 16] = [
        "G", "A", "M", "E", "O", "V", "R", "S", "C", "H", "I", "T", "P", "?", "U", "D",
    ];
    pub(super) const NUMBER: &Tag = FONT_SPRITES.tags().get("Number");

    // Load background tiles as `bg_tiles` module
    agb::include_background_gfx!(bg_tiles, tiles => "assets/gfx/dino_background.bmp");
    const TILE_MAP_CSV_STR: &str = include_str!("../assets/tilemap/dino_map.csv");

    pub const BG_TILES_DATA: TileData = bg_tiles::tiles;
    pub const BG_PALETTES: &[Palette16] = bg_tiles::PALETTES;

    pub fn create_tile_map() -> Vec<usize> {
        TILE_MAP_CSV_STR
            .split([',', '\r', '\n'])
            .map(|s| usize::from_str_radix(s, 10).unwrap_or(0))
            .collect()
    }
    pub(super) fn create_char_sprite_map() -> HashMap<char, &'static Sprite> {
        let mut map: HashMap<char, &'static Sprite> = HashMap::new();
        for sprite_key in CHAR_SPRITE_KEYS {
            let sprite = FONT_SPRITES.tags().get(sprite_key).sprite(0);
            map.insert(sprite_key.chars().next().unwrap(), sprite);
        }
        map
    }

    pub(super) enum SoundEffectKind {
        Jump,
        Over,
        Up,
    }
    pub(super) const JUMP_SOUND: &[u8] = include_bytes!("../assets/sfx/jump.raw"); // include_wav!("assets/sfx/jump.wav");
    pub(super) const OVER_SOUND: &[u8] = include_bytes!("../assets/sfx/over.raw");
    pub(super) const UP_SOUND: &[u8] = include_bytes!("../assets/sfx/up.raw");

    pub(super) fn get_sound(kind: SoundEffectKind) -> SoundChannel {
        let data: &'static [u8] = match kind {
            SoundEffectKind::Jump => JUMP_SOUND,
            SoundEffectKind::Over => OVER_SOUND,
            SoundEffectKind::Up => UP_SOUND,
        };
        SoundChannel::new(data)
    }

    pub const DINO_COLLISION_RECT: Rect<u16> = Rect::<u16> {
        position: Vector2D::new(9, 4),
        size: Vector2D::new(18, 27),
    };
    pub const BIRD_COLLISION_RECT: Rect<u16> = Rect::<u16> {
        position: Vector2D::new(1, 13),
        size: Vector2D::new(28, 7),
    };
    pub const CACTUS_COLLISION_RECT: Rect<u16> = Rect::<u16> {
        position: Vector2D::new(1, 6),
        size: Vector2D::new(27, 25),
    };
    // pub const BG_TILES_WIDTH: u16 = 64;
    pub const BG_TILES_HEIGHT: u16 = 14;
    pub const BG_TILES_OFFSET_Y: u16 = (20 - BG_TILES_HEIGHT) / 2;
    pub const BG_BLANK_TILE_IDX: u16 = 1;
    pub const GROUND_TILE_Y: u16 = 11 + BG_TILES_OFFSET_Y;
    pub const GROUND_Y: u16 = GROUND_TILE_Y * 8 + 2;

    pub const DINO_GROUNDED_Y: u16 = GROUND_Y - 32;
    pub const CACTUS_Y: u16 = GROUND_Y - 32;
}

use crate::{
    game::resource::{
        create_char_sprite_map, BIRD_COLLISION_RECT, CACTUS_COLLISION_RECT, DINO_COLLISION_RECT,
        NUMBER,
    },
    utils::print_info,
};

use self::resource::{
    get_sound, SoundEffectKind, BG_TILES_OFFSET_Y, BIRD, CACTUS, CACTUS_Y, DINO, DINO_GROUNDED_Y,
};

#[derive(Clone)]
pub struct SpriteWithCollisionRect {
    sprite: SpriteVram,
    rect: Rect<u16>,
}

#[derive(Clone)]
pub struct SpriteCache {
    dino: Box<[SpriteWithCollisionRect]>,
    bird: Box<[SpriteWithCollisionRect]>,
    cactus: SpriteWithCollisionRect,
    numbers: Box<[SpriteVram]>,
    char_map: HashMap<char, SpriteVram>,
}

impl SpriteCache {
    pub fn new(loader: &mut SpriteLoader) -> Self {
        fn generate_sprites(
            tag: &'static Tag,
            range: Range<usize>,
            loader: &mut SpriteLoader,
        ) -> Box<[SpriteVram]> {
            range
                .map(|x| tag.sprite(x))
                .map(|x| loader.get_vram_sprite(x))
                .collect::<Vec<_>>()
                .into_boxed_slice()
        }
        fn generate_sprites_with_collision_rect(
            tag: &'static Tag,
            range: Range<usize>,
            loader: &mut SpriteLoader,
            collision_rect: Rect<u16>,
        ) -> Box<[SpriteWithCollisionRect]> {
            range
                .map(|x| tag.sprite(x))
                .map(|x| SpriteWithCollisionRect {
                    sprite: loader.get_vram_sprite(x),
                    rect: collision_rect.clone(),
                })
                .collect::<Vec<_>>()
                .into_boxed_slice()
        }

        let mut char_sprite_vram_map: HashMap<char, SpriteVram> = HashMap::new();
        let char_sprite_map = create_char_sprite_map();
        for (key, sprite) in char_sprite_map.iter() {
            char_sprite_vram_map.insert(*key, loader.get_vram_sprite(sprite));
        }

        Self {
            dino: generate_sprites_with_collision_rect(DINO, 0..3, loader, DINO_COLLISION_RECT),
            bird: generate_sprites_with_collision_rect(BIRD, 0..2, loader, BIRD_COLLISION_RECT),
            cactus: SpriteWithCollisionRect {
                sprite: loader.get_vram_sprite(CACTUS),
                rect: CACTUS_COLLISION_RECT,
            },
            numbers: generate_sprites(NUMBER, 0..10, loader),
            char_map: char_sprite_vram_map,
        }
    }
}

#[derive(Debug)]
struct Player {
    position: Vector2D<Number>,
    vertical_speed: Number,

    is_jumping: bool,
}

#[derive(Debug)]
enum EnemyKind {
    Bird,
    Cactus,
}
#[derive(Debug)]
struct Enemy {
    kind: EnemyKind,
    position: Vector2D<Number>,
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub init_scroll_velocity: Number,

    pub scroll_velocity_increase_per_level: Number,
    pub frames_to_level_up: u32,

    pub animation_interval_frames: u16,
    pub spawn_interval_frames: u16,
    pub jump_height_px: u16,
    pub jump_duration_frames: u16,
    pub max_enemies_displayed: usize,

    pub hi_score: u32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GameState {
    Continue,
    Pause,
    Over(u32),
    Restart,
}

#[derive(Clone, Copy, Debug)]
struct SpawnInfo(u8);
impl From<u8> for SpawnInfo {
    fn from(value: u8) -> Self {
        Self(value)
    }
}
impl SpawnInfo {
    pub fn delay(&self) -> u32 {
        // 0.8 ~ 0.3sec step
        (self.0 & 0b111) as u32 * 12 + 40
    }
    pub fn enemy_kind(&self) -> EnemyKind {
        // 50% bird / 50% cactus
        if ((self.0 & 0b111000) >> 3) < 4 {
            EnemyKind::Bird
        } else {
            EnemyKind::Cactus
        }
    }
    pub fn enemy_arg_2bit(&self) -> u8 {
        (self.0 & 0b11000000) >> 6
    }
}

pub enum TextAlign {
    Left,
    Center,
    Right,
}

pub fn draw_score_digits(
    score: u32,
    position: Vector2D<i32>,
    oam_frame: &mut OamIterator,
    sprite_cache: &SpriteCache,
    align: TextAlign,
) -> Option<()> {
    for digit_pos in 0..6i32 {
        let digit = (score / (10_u32.pow(digit_pos as u32))) % 10;
        let sprite = sprite_cache.numbers.get(digit as usize).unwrap();
        let number_relative_position: i32 = match align {
            TextAlign::Left => 7 * (5 - digit_pos),
            TextAlign::Center => 7 * (2 - digit_pos),
            TextAlign::Right => 7 * (-1 - digit_pos),
        };
        let number_position: Vector2D<i32> =
            (position.x + number_relative_position, position.y).into();

        let mut object = ObjectUnmanaged::new(sprite.clone());
        object.show().set_position(number_position);
        oam_frame.next()?.set(&object);
    }
    Some(())
}
pub fn draw_str(
    str: &'static str,
    position: Vector2D<i32>,
    oam_frame: &mut OamIterator,
    sprite_cache: &SpriteCache,
    align: TextAlign,
) -> Option<()> {
    let uppercase = str.to_uppercase();
    let str_len = str.len();
    for (idx, char) in uppercase.chars().enumerate() {
        if char.is_whitespace() {
            continue;
        }

        let sprite = sprite_cache
            .char_map
            .get(&char)
            .unwrap_or(sprite_cache.char_map.get(&'?').unwrap());

        let mut object = ObjectUnmanaged::new(sprite.clone());
        let char_relative_position: i32 = match align {
            TextAlign::Left => 7 * idx as i32,
            TextAlign::Center => 7 * (idx as i32 - str_len as i32 / 2),
            TextAlign::Right => 7 * (idx as i32 - str_len as i32),
        };

        object
            .show()
            .set_position((position.x + char_relative_position, position.y).into());
        oam_frame.next()?.set(&object);
    }

    Some(())
}

fn play_sound(mixer: &mut Mixer, kind: SoundEffectKind) {
    let mut sound = get_sound(kind);
    sound.volume(num!(0.5));
    mixer.play_sound(sound);
}

pub struct Game {
    mgba: Option<Mgba>,
    settings: Settings,
    state: GameState,
    frame_count: u32,
    speed_level: u16,
    background_position: Vector2D<Number>,
    scroll_velocity: Number,
    gravity_px_per_square_frame: Number,
    input: ButtonController,
    player: Player,
    enemies: VecDeque<Enemy>,
    frames_current_level: u32,
    frames_since_last_spawn: u32,
    spawn_queue: VecDeque<SpawnInfo>,
}

fn frame_ranger(count: u32, start: u32, end: u32, delay: u32) -> usize {
    (((count / delay) % (end + 1 - start)) + start) as usize
}

impl Game {
    pub fn from_settings(settings: Settings) -> Self {
        let player = Player {
            position: (16, DINO_GROUNDED_Y as i32).into(),
            vertical_speed: Number::new(0),
            is_jumping: false,
        };
        let gravity_px_per_square_frame: Number = Number::new(2 * settings.jump_height_px as i32)
            / Number::new(settings.jump_duration_frames.pow(2) as i32);

        Self {
            mgba: Mgba::new(),
            frame_count: 0,
            frames_current_level: 0,
            frames_since_last_spawn: 0,
            speed_level: 0,
            background_position: (0, 0).into(),
            scroll_velocity: settings.init_scroll_velocity,
            input: agb::input::ButtonController::new(),
            player,
            enemies: VecDeque::with_capacity(settings.max_enemies_displayed),
            gravity_px_per_square_frame,
            settings,
            state: GameState::Continue,
            spawn_queue: VecDeque::with_capacity(4),
        }
    }

    fn current_score(&self) -> u32 {
        if self.frame_count < 6000000 {
            self.frame_count / 6
        } else {
            999999
        }
    }

    pub fn frame(
        &mut self,
        sprite_cache: &SpriteCache,
        vram: &mut VRamManager,
        background: &mut InfiniteScrolledMap<'_>,
        mixer: &mut Mixer<'_>,
    ) -> GameState {
        self.input.update();

        if self.input.is_just_pressed(Button::START) {
            match self.state {
                GameState::Continue => {
                    self.state = GameState::Pause;
                    return self.state;
                }
                GameState::Pause => {
                    self.state = GameState::Continue;
                    return self.state;
                }
                _ => {}
            };
        }

        match self.state {
            GameState::Over(_) => {
                if self.input.is_just_pressed(Button::A)
                    || self.input.is_just_pressed(Button::START)
                {
                    // reset game
                    self.state = GameState::Restart;
                }
                return self.state;
            }
            GameState::Pause => {
                return self.state;
            }
            _ => {}
        }

        self.frame_count += 1;
        self.frames_current_level += 1;
        self.frames_since_last_spawn += 1;

        // Update random spawn info
        if self.spawn_queue.is_empty() {
            let rnd = agb::rng::gen() as u32;
            for i in 0..4 {
                let spawn_info = SpawnInfo::from(((rnd >> (i * 8)) & 0xFF) as u8);
                self.spawn_queue.push_back(spawn_info);
            }
        }

        // Process level up
        if self.frames_current_level >= self.settings.frames_to_level_up {
            print_info(
                &mut self.mgba,
                format_args!("level up: {}", self.speed_level + 1),
            );
            play_sound(mixer, SoundEffectKind::Up);

            self.scroll_velocity += self.settings.scroll_velocity_increase_per_level;
            self.speed_level += 1;
            self.frames_current_level = 0;
        }

        // Calc player position
        if self.player.is_jumping {
            self.player.position.y += self.player.vertical_speed;
            let player_y_px = self.player.position.y.floor();
            if player_y_px >= DINO_GROUNDED_Y as i32 {
                self.player.position.y = Num::new(DINO_GROUNDED_Y as i32);
                self.player.is_jumping = false;
            }
            self.player.vertical_speed += self.gravity_px_per_square_frame;
        } else if self.input.is_just_pressed(Button::A) {
            play_sound(mixer, SoundEffectKind::Jump);

            self.player.vertical_speed =
                -self.gravity_px_per_square_frame * (self.settings.jump_duration_frames as i32);
            self.player.is_jumping = true;
        }

        // Spawn enemy
        if self.frames_since_last_spawn > self.spawn_queue.front().unwrap().delay() {
            let spawn_info = self.spawn_queue.pop_front().unwrap();
            print_info(
                &mut self.mgba,
                format_args!(
                    "[T={}, dt={}] spawn: {} {:?} {}",
                    self.frame_count,
                    self.frames_since_last_spawn,
                    spawn_info.delay(),
                    spawn_info.enemy_kind(),
                    spawn_info.enemy_arg_2bit()
                ),
            );
            self.frames_since_last_spawn = 0;

            if self.enemies.len() < self.enemies.capacity() {
                let enemy = match spawn_info.enemy_kind() {
                    EnemyKind::Bird => {
                        let spawn_y = (spawn_info.enemy_arg_2bit() as i32 + 6) * 8;
                        Enemy {
                            kind: EnemyKind::Bird,
                            position: (8 * 30, spawn_y).into(),
                        }
                    }
                    EnemyKind::Cactus => {
                        // let n_cactuses = spawn_info.enemy_arg() & 0b1 + 1;
                        Enemy {
                            kind: EnemyKind::Cactus,
                            position: (8 * 30, CACTUS_Y as i32).into(),
                        }
                    }
                };
                self.enemies.push_back(enemy);
            }
        }

        // Calc enemies' position and collision detection
        let mut player_collision_rect = sprite_cache.dino.get(0).unwrap().rect;
        player_collision_rect.position += (
            self.player.position.x.floor() as u16,
            self.player.position.y.floor() as u16,
        )
            .into();
        let mut total_enemies_out: usize = 0;
        let mut is_collided: bool = false;
        for enemy in self.enemies.iter_mut() {
            if enemy.position.x.floor() < -32 {
                total_enemies_out += 1;
            } else {
                enemy.position.x -= self.scroll_velocity;

                // Collision detection
                if self.player.position.x <= enemy.position.x + 32
                    && enemy.position.x <= self.player.position.x + 32
                {
                    let mut enemy_collision_rect = match enemy.kind {
                        EnemyKind::Bird => sprite_cache.bird.get(0).unwrap().rect,
                        EnemyKind::Cactus => sprite_cache.cactus.rect,
                    };
                    enemy_collision_rect.position += (
                        enemy.position.x.floor() as u16,
                        enemy.position.y.floor() as u16,
                    )
                        .into();

                    if enemy_collision_rect.touches(player_collision_rect) {
                        print_info(&mut self.mgba, format_args!("collide: {:?}", enemy.kind));
                        is_collided = true;
                    }
                }
            };
        }
        if is_collided {
            play_sound(mixer, SoundEffectKind::Over);
            self.state = GameState::Over(self.current_score());
        }

        // Remove first n enemies which are out of screen
        self.enemies.drain(..total_enemies_out);

        self.background_position.x += self.scroll_velocity;
        background.set_pos(vram, self.background_position.floor());
        self.state
    }

    pub fn render(
        &mut self,
        oam_frame: &mut OamIterator,
        sprite_cache: &SpriteCache,
    ) -> Option<()> {
        let sprite_index: usize = frame_ranger(
            self.frame_count,
            0,
            1,
            self.settings.animation_interval_frames as u32,
        );

        // Draw player
        let sprite = match self.state {
            GameState::Over(_) => sprite_cache.dino.get(2).unwrap().sprite.clone(),
            _ => {
                if self.player.is_jumping {
                    sprite_cache.dino.get(1).unwrap().sprite.clone()
                } else {
                    sprite_cache.dino.get(sprite_index).unwrap().sprite.clone()
                }
            }
        };
        let mut player_object = ObjectUnmanaged::new(sprite);
        player_object
            .show()
            .set_position(self.player.position.floor());
        oam_frame.next()?.set(&player_object);

        // Draw enemy
        for enemy in self.enemies.iter() {
            let sprite = match enemy.kind {
                EnemyKind::Bird => sprite_cache.bird.get(sprite_index).unwrap().sprite.clone(),
                EnemyKind::Cactus => sprite_cache.cactus.sprite.clone(),
            };
            let mut object = ObjectUnmanaged::new(sprite);
            object.show().set_position(enemy.position.floor());
            oam_frame.next()?.set(&object);
        }

        // Draw score
        let score = self.current_score();
        let score_value_right = 236;
        let score_y = (BG_TILES_OFFSET_Y * 8 - 9) as i32;
        draw_score_digits(
            score,
            (score_value_right, score_y).into(),
            oam_frame,
            sprite_cache,
            TextAlign::Right,
        );
        draw_str(
            "SCORE",
            (score_value_right - 7 * 6 - 2, score_y + 1).into(),
            oam_frame,
            sprite_cache,
            TextAlign::Right,
        );

        // Draw hi score
        let hi_score_y = (BG_TILES_OFFSET_Y * 8 - 18) as i32;
        draw_score_digits(
            self.settings.hi_score,
            (score_value_right, hi_score_y).into(),
            oam_frame,
            sprite_cache,
            TextAlign::Right,
        );
        draw_str(
            "HI",
            (score_value_right - 7 * 6 - 2, hi_score_y + 1).into(),
            oam_frame,
            sprite_cache,
            TextAlign::Right,
        );

        match self.state {
            GameState::Over(_) => {
                draw_str(
                    "G A M E  O V E R",
                    (120, 60).into(),
                    oam_frame,
                    sprite_cache,
                    TextAlign::Center,
                );
                draw_str(
                    "PRESS A TO RESTART",
                    (120, 75).into(),
                    oam_frame,
                    sprite_cache,
                    TextAlign::Center,
                );
            }
            GameState::Pause => {
                draw_str(
                    "P A U S E D",
                    (120, 60).into(),
                    oam_frame,
                    sprite_cache,
                    TextAlign::Center,
                );
                draw_str(
                    "PRESS START TO RESUME",
                    (120, 75).into(),
                    oam_frame,
                    sprite_cache,
                    TextAlign::Center,
                );
            }
            _ => {}
        }

        Some(())
    }
}

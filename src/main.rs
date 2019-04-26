//Rusty Rogue, a roguelike by pard68
//This game follows the tcod-rs tutorial series

extern crate tcod;
extern crate rand;

use std::cmp;

use rand::Rng;

use tcod::console::*;
use tcod::colors::{self, Color};
use tcod::map::{Map as FovMap, FovAlgorithm};

//CONSTANTS

const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

const BAR_WIDTH: i32 = 20;
const PANEL_HEIGHT: i32 = 7;
const PANEL_Y: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 43;

const COLOR_DARK_WALL: Color = Color { r: 0, g: 0, b: 100 };
const COLOR_LIGHT_WALL: Color = Color { r: 130, g: 110, b: 50 };
const COLOR_DARK_GROUND: Color = Color { r: 50, g: 50, b: 150 };
const COLOR_LIGHT_GROUND: Color = Color { r: 200, g: 180, b: 50 };

const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic;
const FOV_LIGHT_WALLS: bool = true;
const TORCH_RADIUS: i32 = 10;

const PLAYER: usize = 0;

const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;
const MAX_ROOM_MONSTERS: i32 = 3;

const LIMIT_FPS: i32 = 20;

//TYPES

type Map = Vec<Vec<Tile>>;

//STRUCTS

#[derive(Clone, Copy, Debug)]
struct Tile {
    blocked: bool,
    block_sight: bool,
    explored: bool,
}

impl Tile {
    pub fn empty() -> Self {
        Tile{blocked: false, block_sight: false, explored: false}
    }

    pub fn wall() -> Self {
        Tile{blocked: true, block_sight: true, explored: false}
    }
}

#[derive(Debug)]
struct Object {
    x: i32,
    y: i32,
    char: char,
    color: Color,
    name: String,
    blocks: bool,
    alive: bool,
    fighter: Option<Fighter>,
    ai: Option<Ai>,
}

impl Object {
    pub fn new(x: i32, y: i32, char: char, name: &str, color: Color, blocks: bool) -> Self {
        Object {
            x: x,
            y: y,
            char: char,
            color: color,
            name: name.into(),
            blocks: blocks,
            alive: false,
            fighter: None,
            ai: None,
        }
    }

    pub fn draw(&self, con: &mut Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.char, BackgroundFlag::None);
    }

    pub fn clear(&self, con: &mut Console) {
        con.put_char(self.x, self.y, ' ', BackgroundFlag::None);
    }

    pub fn pos(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn set_pos(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    pub fn distance_to(&self, other: &Object) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        ((dx.pow(2) + dy.pow(2)) as f32).sqrt()
    }

    pub fn take_damage(&mut self, damage: i32) {
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp -= damage;
            }
        }

        if let Some(fighter) = self.fighter {
            if fighter.hp <= 0 {
                self.alive = false;
                fighter.on_death.callback(self);
            }
        }
    }

    pub fn attack(&mut self, target: &mut Object) {
        let damage = self.fighter.map_or(0, |f| f.power) - target.fighter
                                                .map_or(0, |f| f.defense);
        if damage > 0 {
            println!("{} attacks {} for {} hit points!",
                     self.name, target.name, damage);
            target.take_damage(damage);
        } else {
            println!("{} attacks {} but the attack has no effect!",
                     self.name, target.name);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x1: i32,
    x2: i32,
    y1: i32,
    y2: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x1: x, y1: y, x2: x + w, y2: y + h }
    }

    pub fn center(&self) -> (i32, i32) {
        let center_x = (self.x1 + self.x2) / 2;
        let center_y = (self.y1 + self.y2) / 2;
        (center_x, center_y)
    }

    pub fn intersects_with(&self, other: &Rect) -> bool {
        (self.x1 <= other.x2) && (self.x2 >= other.x1) &&
            (self.y1 <= other.y2) && (self.y2 >= other.y1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Fighter {
    max_hp: i32,
    hp: i32,
    defense: i32,
    power: i32,
    on_death: DeathCallback,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DeathCallback {
    Player,
    Monster,
}

impl DeathCallback {
    fn callback(self, object: &mut Object) {
        use DeathCallback::*;
        let callback: fn(&mut Object) = match self {
            Player => player_death,
            Monster => monster_death,
        };
        callback(object);
    } 
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Ai;

//FUNCTIONS

fn move_by(id: usize, dx: i32, dy: i32, map: &Map, objects: &mut [Object]) {
    let (x, y) = objects[id].pos();
    if !is_blocked(x + dx, y + dy, map, objects) {
        objects[id].set_pos(x + dx, y + dy);
    }
}

fn create_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    for x in cmp::min(x1, x2)..(cmp::max(x1, x2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn create_v_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    for y in cmp::min(y1, y2)..(cmp::max(y1, y2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn create_room(room: Rect, map: &mut Map) {
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            map[x as usize][y as usize] = Tile::empty();
        }
    }
}

fn make_map(objects: &mut Vec<Object>) -> Map {
    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];

    let mut rooms = vec![];

    for _ in 0..MAX_ROOMS {
        let w = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let h = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let x = rand::thread_rng().gen_range(0, MAP_WIDTH - w);
        let y = rand::thread_rng().gen_range(0, MAP_HEIGHT - h);

        let new_room = Rect::new(x, y, w, h);

        let failed = rooms.iter().any(|other_room| new_room.intersects_with(other_room));

        if !failed {
            create_room(new_room, &mut map);
            place_objects(new_room, objects);

            let (new_x, new_y) = new_room.center();
            if rooms.is_empty() {
                objects[PLAYER].set_pos(new_x, new_y);
            } else {
                let (prev_x, prev_y) = rooms[rooms.len() -1].center();
                if rand::random() {
                    create_h_tunnel(prev_x, new_x, prev_y, &mut map);
                    create_v_tunnel(prev_y, new_y, new_x, &mut map);
                } else {
                    create_v_tunnel(prev_y, new_y, prev_x, &mut map);
                    create_h_tunnel(prev_x, new_x, new_y, &mut map);
                }
            }
        }
        rooms.push(new_room);
    }
    map
}

fn render_all(root: &mut Root, con: &mut Offscreen, objects: &[Object],
              map: &mut Map, fov_map: &mut FovMap, fov_recompute: bool, panel: &mut Offscreen) {
    if fov_recompute {
        let player = &objects[PLAYER];
        fov_map.compute_fov(player.x, player.y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);
    }

    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let visible = fov_map.is_in_fov(x, y);
            let wall = map[x as usize][y as usize].block_sight;
            let color = match (visible, wall) {
                (false, true) => COLOR_DARK_WALL,
                (false, false) => COLOR_DARK_GROUND,
                (true, true) => COLOR_LIGHT_WALL,
                (true, false) => COLOR_LIGHT_GROUND,
            };
            let explored = &mut map[x as usize][y as usize].explored;
            if visible {
                *explored = true;
            }
            if *explored {
                con.set_char_background(x, y, color, BackgroundFlag::Set);
            }
        }
    }

    let mut to_draw: Vec<_> = objects.iter().filter(|o| fov_map.is_in_fov(o.x, o.y))
                                .collect();
    to_draw.sort_by(|o1, o2| {o1.blocks.cmp(&o2.blocks) });
    for object in &to_draw {
        object.draw(con);
        }
    blit(con, (0, 0), (MAP_WIDTH, MAP_HEIGHT), root, (0, 0), 1.0, 1.0);

    panel.set_default_background(colors::BLACK);
    panel.clear();
    let hp = objects[PLAYER].fighter.map_or(0, |f| f.hp);
    let max_hp = objects[PLAYER].fighter.map_or(0, |f| f.max_hp);
    render_bar(panel, 1, 1, BAR_WIDTH, "HP", hp, max_hp, colors::LIGHT_RED, colors::DARKER_RED);

    blit(panel, (0, 0), (SCREEN_WIDTH, PANEL_HEIGHT), root, (0, PANEL_Y), 1.0, 1.0);
}

fn player_move_or_attack(dx: i32, dy: i32, map: &Map, objects: &mut [Object]) {
    let x = objects[PLAYER].x + dx;
    let y = objects[PLAYER].y + dy;

    let target_id = objects.iter().position(|object| {
        object.fighter.is_some() && object.pos() == (x, y)
    });

    match target_id {
        Some(target_id) => {
            let (player, target) = mut_two(PLAYER, target_id, objects);
            player.attack(target);
        }
        None => {
            move_by(PLAYER, dx, dy, map, objects);
        }
    }
}

fn handle_keys(root: &mut Root, objects: &mut [Object], map: &Map) -> PlayerAction {
    use tcod::input::Key;
    use tcod::input::KeyCode::*;
    use PlayerAction::*;

    let key = root.wait_for_keypress(true);
    let player_alive = objects[PLAYER].alive;
    match (key, player_alive) {
        //movement keys
        (Key { code: Up, .. }, true) => {
            player_move_or_attack(0, -1, &map, objects);
            TookTurn
        }
        (Key { code: Down, .. }, true) => {
            player_move_or_attack(0, 1, &map, objects);
            TookTurn
        }
        (Key { code: Left, .. }, true) => {
            player_move_or_attack(-1, 0, &map, objects);
            TookTurn
        }
        (Key { code: Right, .. }, true) => {
            player_move_or_attack(1, 0, &map, objects);
            TookTurn
        }

        //Fullscreen
        (Key { code: Enter, alt: true, .. }, _) => {
            let fullscreen = root.is_fullscreen();
            root.set_fullscreen(!fullscreen);
            DidntTakeTurn
        }

        //Exit
        (Key { code: Escape, .. }, _) => Exit,

        //Catchall
        _ => DidntTakeTurn,
    }
}

fn place_objects(room: Rect, objects: &mut Vec<Object>) {
    use DeathCallback::*;
    let num_monsters = rand::thread_rng().gen_range(0, MAX_ROOM_MONSTERS + 1);

    for _ in 0..num_monsters {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        let mut monster = if rand::random::<f32>() < 0.8 {
            let mut orc = Object::new(x, y, 'o', "orc", colors::DESATURATED_GREEN, true);
            orc.fighter = Some(Fighter{max_hp: 10, hp: 10, defense: 1, power: 3, on_death: Monster});
            orc.ai = Some(Ai);
            orc
        } else {
            let mut troll = Object::new(x, y, 'T', "troll", colors::DARKER_GREEN, true);
            troll.fighter = Some(Fighter{max_hp: 16, hp: 16, defense: 1, power: 4, on_death: Monster});
            troll.ai = Some(Ai);
            troll
        };
        monster.alive = true;
        objects.push(monster)
    }
}

fn is_blocked(x: i32, y: i32, map: &Map, objects: &[Object]) -> bool {
    if map[x as usize][y as usize].blocked {
        return true;
    }

    objects.iter().any(|object| {
        object.blocks && object.pos() == (x, y)
    })
}

fn move_towards(id: usize, target_x: i32, target_y: i32, map: &Map, objects: &mut [Object]) {
    let dx = target_x - objects[id].x;
    let dy = target_y - objects[id].y;
    let distance = ((dx.pow(2) + dy.pow(2)) as f32).sqrt();

    let dx = (dx as f32 / distance).round() as i32;
    let dy = (dy as f32 / distance).round() as i32;
    move_by(id, dx, dy, map, objects);
}

fn ai_take_turn(monster_id: usize, map: &Map, objects: &mut [Object], fov_map: &FovMap) {
    let (monster_x, monster_y) = objects[monster_id].pos();
    if fov_map.is_in_fov(monster_x, monster_y) {
        if objects[monster_id].distance_to(&objects[PLAYER]) >= 2.0 {
            let (player_x, player_y) = objects[PLAYER].pos();
            move_towards(monster_id, player_x, player_y, map, objects);
        } else {
            let (monster, player) = mut_two(monster_id, PLAYER, objects);
            monster.attack(player);
        }
    }
}

fn mut_two<T>(first_index: usize, second_index: usize, items: &mut [T]) -> (&mut T, &mut T) {
    assert!(first_index != second_index);
    let split_at_index = cmp::max(first_index, second_index);
    let (first_slice, second_slice) = items.split_at_mut(split_at_index);
    if first_index < second_index {
        (&mut first_slice[first_index], &mut second_slice[0])
    } else {
        (&mut second_slice[0], &mut first_slice[second_index])
    }
}

fn player_death(player: &mut Object) {
    println!("You died!");
    player.char = '%';
    player.color = colors::DARK_RED;
}

fn monster_death(monster: &mut Object) {
    println!("{} is dead!", monster.name);
    monster.char = '%';
    monster.color = colors::DARK_RED;
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("remains of {}", monster.name);
}

fn render_bar(panel: &mut Offscreen,
              x: i32, y: i32,
              total_width: i32,
              name: &str, value: i32,
              maximum: i32,
              bar_color: Color,
              back_color: Color)
{
    let bar_width = (value as f32 / maximum as f32 * total_width as f32) as i32;
    panel.set_default_background(back_color);
    panel.rect(x, y, total_width, 1, false, BackgroundFlag::Screen);

    panel.set_default_background(bar_color);
    if bar_width > 0 {
        panel.rect(x, y, bar_width, 1, false, BackgroundFlag::Screen);
    }

    panel.set_default_foreground(colors::WHITE);
    panel.print_ex(x + total_width / 2, y, BackgroundFlag::None, TextAlignment::Center,
                   &format!("{}: {}/{}", name, value, maximum));
}

fn main() {
    let mut root = Root::initializer()
        .font("static/arial10x10.png", FontLayout::Tcod)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Rusty Rogue")
        .init();

    let mut con = Offscreen::new(MAP_WIDTH, MAP_HEIGHT);
    let mut panel = Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT);

    tcod::system::set_fps(LIMIT_FPS);
    
    use DeathCallback::*;
    let mut player = Object::new(0, 0, '@', "player", colors::WHITE, true);
    player.alive = true;
    player.fighter = Some(Fighter{max_hp: 30, hp: 30, defense: 2, power: 5, on_death: Player});

    let mut objects = vec![player];

    let mut map = make_map(&mut objects);

    let mut fov_map = FovMap::new(MAP_WIDTH, MAP_HEIGHT);
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            fov_map.set(x, y,
                        !map[x as usize][y as usize].block_sight,
                        !map[x as usize][y as usize].blocked);
        }
    }

    let mut previous_player_position = (-1, -1);

    while !root.window_closed() {
        con.clear();

        let fov_recompute = previous_player_position != objects[PLAYER].pos();
        render_all(&mut root, &mut con, &objects, &mut map, &mut fov_map, fov_recompute, &mut panel);

        root.flush();

        let player = &mut objects[PLAYER];
        previous_player_position = (player.x, player.y);
        let player_action = handle_keys(&mut root, &mut objects, &map);
        if player_action == PlayerAction::Exit {
            break
        }

        if objects[PLAYER].alive && player_action != PlayerAction::DidntTakeTurn {
                    for id in 0..objects.len() {
                        if objects[id].ai.is_some() {
                            ai_take_turn(id, &map, &mut objects, &fov_map);
                }
            }
        }
    };
}

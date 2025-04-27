use itertools::{Itertools, iproduct};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{Debug, DebugSet, Display, Formatter, write};
use std::hash::{Hash, Hasher};
use std::num::ParseIntError;
use thiserror::Error;

const MAXPOSITION: u32 = 10;

#[derive(Error, Debug)]
enum MyError {
    // failed to add
    #[error("Could not add item {0}")]
    FailedAdd(Item),
    // failed to remove
    #[error("Could not remove item with Id {0}")]
    FailedRemove(u32),
    // got filtered
    #[error("The item {0} triggered some filter")]
    BlockedByFilter(Item),
    // failed to find an alloc
    #[error("The allocator could not find a position for item {0}")]
    FailedAllocation(Item),
    // IO error
    #[error("IO error: {0}")]
    IOError(std::io::Error),
    // Parse error
    #[error("Parse int error: {0}")]
    ParseIntError(ParseIntError),
    //Invalid Error Format
    #[error("Invalid Date Format: {0}")]
    InvalidDateFormat(String),
    //Wrong Option
    #[error("Wrong Option: {0}")]
    WrongOption(String),
}

trait Filter: Debug {
    fn check_allowed(&self, item: &Item, map: &HashMap<Position, Option<Item>>) -> bool;
}

trait Strategy: Debug {
    fn allocate(&mut self, item: &Item, map: &HashMap<Position, Option<Item>>) -> Option<Position>;
}

#[derive(Copy, Clone, Debug)]
struct Position {
    row: u32,
    shelf: u32,
    zone: u32,
    occupied: bool,
}

impl Position {
    fn new(row: u32, shelf: u32, zone: u32) -> Position {
        Position {
            row,
            shelf,
            zone,
            occupied: false,
        }
    }

    fn as_tuple(&self) -> (u32, u32, u32) {
        (self.row, self.shelf, self.zone)
    }
}

impl PartialEq for Position {
    fn eq(&self, other: &Position) -> bool {
        self.row == other.row && self.shelf == other.shelf && self.zone == other.zone
        // doesnt take self.occupied into account!
    }
}

impl Eq for Position {}

impl Hash for Position {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.row.hash(state);
        self.shelf.hash(state);
        self.zone.hash(state);
        // doesnt take self.occupied into account!
    }
}

impl From<(u32, u32, u32)> for Position {
    fn from(pos: (u32, u32, u32)) -> Position {
        let (row, shelf, zone) = pos;
        Position {
            row,
            shelf,
            zone,
            occupied: false,
        }
    }
}

impl Display for Position {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.row, self.shelf, self.zone)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum Quality {
    Fragile { expiration_date: [u32; 3], row: u32 },
    Oversized { continuous_zones: u32 },
    Normal,
}

impl Display for Quality {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Quality::Fragile {
                expiration_date,
                row,
            } => {
                write!(
                    f,
                    "Fragile (Expiration: {:?}, Row: {})",
                    expiration_date, row
                )
            }
            Quality::Oversized { continuous_zones } => {
                write!(f, "Oversized (Continuous Zones: {})", continuous_zones)
            }
            Quality::Normal => {
                write!(f, "Normal")
            }
        }
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
struct Item {
    id: u32,
    name: String,
    quantity: u32,
    quality: Quality,
}

impl Display for Item {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} - {}, quantity: {}, quality: {}",
            self.id, self.name, self.quantity, self.quality
        )
    }
}

#[derive(Debug)]
struct Placement {
    map: HashMap<Position, Option<Item>>,
    allocation_strategy: Box<dyn Strategy>,
    id_map: HashMap<u32, Item>, // given an item ID, return me the ITEM
    name_map: HashMap<String, Item>, // given an item NAME, return me the ITEM
    position_map: HashMap<u32, Vec<Position>>, // given an item ID, return me all positions
    filter_list: Vec<Box<dyn Filter>>,
}
impl Placement {
    fn new() -> Placement {
        // pre-generate all positions
        let mut map = HashMap::new();
        let mut id_map = HashMap::new();
        let mut name_map = HashMap::new();
        let mut position_map = HashMap::new();

        for (i, j, k) in iproduct!(0..MAXPOSITION, 0..MAXPOSITION, 0..MAXPOSITION) {
            map.insert(Position::from((i, j, k)), None);
        }

        Placement {
            map,
            allocation_strategy: Box::from(RoundRobin {}),
            id_map,
            name_map,
            position_map,
            filter_list: Vec::new(),
        }
    }

    fn configure_filters(&mut self, list: Vec<Box<dyn Filter>>) {
        self.filter_list = list
    }

    fn is_allowed_by_filters(&self, item: &Item) -> bool {
        self.filter_list
            .iter()
            .all(|filt| filt.check_allowed(item, &self.map))
    }

    fn add_item(&mut self, item: Item) -> Result<(), MyError> {
        if !self.is_allowed_by_filters(&item) {
            return Err(MyError::BlockedByFilter(item));
        }

        let mut position = match self.allocation_strategy.allocate(&item, &self.map) {
            Some(position) => position,
            None => return Err(MyError::FailedAllocation(item)),
        };
        position.occupied = true;

        self.id_map.insert(item.id.clone(), item.clone());
        self.name_map.insert(item.name.clone(), item.clone());

        match &item.quality {
            Quality::Normal | Quality::Fragile { .. } => {
                self.map.remove(&position); // remove old key with OCCUPIED = false
                self.map.insert(position, Some(item.clone())); // add with OCCUPIED = true
                self.position_map.insert(item.id, vec![position]);
            }
            Quality::Oversized { continuous_zones } => {
                for k in position.zone..(position.zone + continuous_zones) {
                    let mut temp = Position::new(position.row, position.shelf, k);
                    temp.occupied = true;
                    if k == position.zone {
                        self.map.remove(&temp); // remove old key with OCCUPIED = false
                        self.map.insert(temp, Some(item.clone())); // add with OCCUPIED = true
                        self.position_map.insert(item.id, vec![position]);
                    } else {
                        self.map.remove(&temp); // remove old key with OCCUPIED = false
                        self.map.insert(temp, None); // add with OCCUPIED = true
                        // BUT! No item, since it is occupied by a Oversized item in an earlier position

                        if let Some(positions) = self.position_map.get_mut(&item.id) {
                            positions.push(temp);
                        }
                    }
                }
            }
        }

        let test = self.map.get(&position);
        if test.is_none() {
            return Err(MyError::FailedAdd(item.clone()))
        }
        Ok(())
    }

    fn remove_item(&mut self, id: u32) -> Result<(), MyError> {
        /*
           if let Some((position, item)) = self.map {
               match &item.quality {
                   Quality::Normal | Quality::Fragile { .. } => {
                       let mut temp = position;
                       temp.occupied = false;

                       self.map.insert(temp, None); // remove old Key with OCCUPIED = false
                   }
                   Quality::Oversized { continuous_zones } => {
                       for k in position.zone..(position.zone + continuous_zones) {
                           let mut temp = Position::new(position.row, position.shelf, k);
                           temp.occupied = false;

                           if k == position.zone {
                               self.map.insert(temp, None); // remove old Key with OCCUPIED = false
                           } else {
                               self.map.insert(temp, None);
                           }
                       }
                   }
               }
           }

        */
        // check if ID exists, else errors out
        let name_ref = match self.id_map.get(&id) {
            Some(item) => item,
            None => return Err(MyError::FailedRemove(id)),
        };

        let existing_positions = self.position_map.get(&id);
        for pos in existing_positions.into_iter().flatten() {
            let mut tmp = Position::from((pos.row, pos.shelf, pos.zone));
            tmp.occupied = false;
            self.map.remove(&tmp); // remove old KEY with OCCUPIED = true
            self.map.insert(tmp, None); // add new KEY with OCCUPIED = false
        }
        let name_ref = &name_ref.name;
        self.name_map.remove(name_ref);
        self.id_map.remove(&id);
        self.position_map.remove(&id);
        Ok(())
    }

    fn alphabetical(&self) -> Vec<Item> {
        let list = self
            .map
            .values()
            .filter_map(|v| v.clone())
            .collect::<Vec<Item>>();
        let sorted_list = list
            .iter()
            .sorted_by(|a, b| Ord::cmp(&a.name.to_lowercase(), &b.name.to_lowercase()))
            .cloned()
            .collect();
        sorted_list
    }

    fn id_search(&mut self, search_id: u32) -> Option<&Item> {
        self.id_map.get(&search_id)
    }
    fn name_search(&mut self, search_name: String) -> Option<&Item> {
        self.name_map.get(&search_name)
    }

    fn check_expired_products(&self, expiration_date: [u32; 3]) -> Option<HashSet<Item>> {
        let [current_day, current_month, current_year] = expiration_date;
        let mut expired_items = HashSet::new();

        for (_, opt_item) in &self.map {
            if let Some(item) = opt_item {
                if let Quality::Fragile {
                    expiration_date: item_expiration_date,
                    ..
                } = &item.quality
                {
                    let item_day = item_expiration_date[0];
                    let item_month = item_expiration_date[1];
                    let item_year = item_expiration_date[2];

                    if current_year > item_year
                        || (current_year == item_year && current_month > item_month)
                        || (current_year == item_year
                            && current_month == item_month
                            && current_day >= item_day)
                    {
                        expired_items.insert(item.clone());
                    }
                }
            }
        }

        if expired_items.is_empty() {
            None
        } else {
            Some(expired_items)
        }
    }

    fn position_search(&mut self, id: u32) -> Option<Vec<Position>> {
        self.position_map.get(&id).cloned()
    }
}

impl Display for Placement {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for (key, opt) in self.map.iter() {
            if let Some(item) = opt {
                write!(f, "{} -> {}\n", key, item)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
struct RoundRobin {}

impl RoundRobin {
    fn is_position_valid(
        &self,
        pos: &Position,
        item: &Item,
        map: &HashMap<Position, Option<Item>>,
    ) -> bool {
        match &item.quality {
            Quality::Fragile {
                expiration_date,
                row,
            } => pos.row < *row,
            Quality::Oversized { continuous_zones } => {
                if pos.zone + continuous_zones > MAXPOSITION {
                    // check if there is enough space
                    false
                } else {
                    // then check if existing space is free (not occupied)
                    let mut flag = true;
                    for k in pos.zone..(pos.zone + continuous_zones) {
                        let pos_test = Position::from((pos.row, pos.shelf, k));
                        let opt = map.get_key_value(&pos_test);
                        match opt {
                            Some((k, _)) => {
                                if k.occupied {
                                    flag = false;
                                    break;
                                }
                            }
                            None => {
                                flag = false;
                                break;
                            }
                        }
                        return flag;
                    }
                    false
                }
            }
            Quality::Normal => true,
        }
    }
}

impl Strategy for RoundRobin {
    fn allocate(&mut self, item: &Item, map: &HashMap<Position, Option<Item>>) -> Option<Position> {
        for (i, j, k) in iproduct!(0..MAXPOSITION, 0..MAXPOSITION, 0..MAXPOSITION) {
            let pos = Position::from((i, j, k));
            let opt = map.get_key_value(&pos);
            // this option holds (Position, Option<Item>) that is actually inside the hashmap
            // now we need to write logic based on Position.occupied
            match opt {
                Some((p, _)) => {
                    if p.occupied {
                        // println!("Yoo {}{}{} is OCCUPIED!! Not worth our time.", i, j, k);
                        continue;
                    } else {
                        if self.is_position_valid(&pos, item, &map) {
                            // lets check if satisfies item quality requirements
                            return Some(p.clone());
                        } else {
                            continue;
                        }
                    }
                }
                None => {
                    continue;
                } // should not ever trigger, but it is here anyway
            }
        }
        None
    }
}

// Two types of filter
// a) Avoid Oversize with too big size
// b) Avoid Fragile with too small max.row
#[derive(Debug)]
struct AvoidTooLarge {
    cutoff: u32,
}

impl Filter for AvoidTooLarge {
    fn check_allowed(&self, item: &Item, map: &HashMap<Position, Option<Item>>) -> bool {
        match &item.quality {
            Quality::Fragile { .. } | Quality::Normal => true,
            Quality::Oversized { continuous_zones } => continuous_zones <= &self.cutoff,
        }
    }
}
#[derive(Debug)]
struct AvoidTooFragile {
    cutoff: u32,
}

impl Filter for AvoidTooFragile {
    fn check_allowed(&self, item: &Item, map: &HashMap<Position, Option<Item>>) -> bool {
        match &item.quality {
            Quality::Oversized { .. } | Quality::Normal => true,
            Quality::Fragile { row, .. } => row >= &self.cutoff,
        }
    }
}

// Ask for info

fn ask_expiration_date() -> Result<[u32; 3], MyError> {
    println!("Insert expiration date as xx-xx-xxxx");

    let mut input_expiration_date: String = String::new();
    let result = std::io::stdin().read_line(&mut input_expiration_date);
    if let Err(err) = result {
        return Err(MyError::IOError(err));
    }
    input_expiration_date = input_expiration_date.trim().to_string();
    let parts: Vec<&str> = input_expiration_date.split('-').map(|s| s.trim()).collect();
    if parts.len() != 3 {
        return Err(MyError::InvalidDateFormat(input_expiration_date));
    }

    let day = parts[0].parse::<u32>().map_err(MyError::ParseIntError)?;
    let month = parts[1].parse::<u32>().map_err(MyError::ParseIntError)?;
    let year = parts[2].parse::<u32>().map_err(MyError::ParseIntError)?;

    let input_expiration_date: [u32; 3] = [day, month, year];
    Ok(input_expiration_date)
}

fn ask_name() -> Result<String, MyError> {
    println!("Name:");
    let mut input_name: String = String::new();
    let result = std::io::stdin().read_line(&mut input_name);
    if let Err(err) = result {
        return Err(MyError::IOError(err));
    };
    input_name = input_name.trim().to_string();
    Ok(input_name)
}

fn ask_id() -> Result<u32, MyError> {
    println!("Id:");
    let mut input_id: String = String::new();
    let result = std::io::stdin().read_line(&mut input_id);
    if let Err(err) = result {
        return Err(MyError::IOError(err));
    }
    input_id = input_id.trim().to_string();
    let result = input_id.parse::<u32>();
    match result {
        Ok(value) => Ok(value),
        Err(err) => Err(MyError::ParseIntError(err)),
    }
}

fn ask_new_product() -> Result<Item, MyError> {
    println!("Id:");
    let mut input_id: String = String::new();
    let result = std::io::stdin().read_line(&mut input_id);
    if let Err(err) = result {
        return Err(MyError::IOError(err));
    }
    input_id = input_id.trim().to_string();

    let result = input_id.parse::<u32>();
    let input_id = match result {
        Ok(duration) => duration,
        Err(err) => return Err(MyError::ParseIntError(err)),
    };

    println!("Name:");
    let mut input_name: String = String::new();
    let result = std::io::stdin().read_line(&mut input_name);
    if let Err(err) = result {
        return Err(MyError::IOError(err));
    };

    println!("Quantity:");
    let mut input_quantity: String = String::new();
    let result = std::io::stdin().read_line(&mut input_quantity);
    if let Err(err) = result {
        return Err(MyError::IOError(err));
    }
    input_quantity = input_quantity.trim().to_string();

    let result = input_quantity.parse::<u32>();
    let input_quantity = match result {
        Ok(duration) => duration,
        Err(err) => return Err(MyError::ParseIntError(err)),
    };

    println!(
        "Quality:\n\
              0: Fragile\n\
              1: Oversized\n\
              2: Normal"
    );
    let mut input_quality: String = String::new();
    let result = std::io::stdin().read_line(&mut input_quality);
    if let Err(err) = result {
        return Err(MyError::IOError(err));
    };

    let product_quality = match input_quality.trim() {
        "0" => {
            println!("Insert expiration date as xx-xx-xxxx");

            let mut input_expiration_date: String = String::new();
            let result = std::io::stdin().read_line(&mut input_expiration_date);
            if let Err(err) = result {
                return Err(MyError::IOError(err));
            }
            input_expiration_date = input_expiration_date.trim().to_string();
            let parts: Vec<&str> = input_expiration_date.split('-').map(|s| s.trim()).collect();
            if parts.len() != 3 {
                return Err(MyError::InvalidDateFormat(input_expiration_date));
            }

            let day = parts[0].parse::<u32>().map_err(MyError::ParseIntError)?;
            let month = parts[1].parse::<u32>().map_err(MyError::ParseIntError)?;
            let year = parts[2].parse::<u32>().map_err(MyError::ParseIntError)?;

            let input_expiration_date: [u32; 3] = [day, month, year];

            println!("Insert max row");

            let mut input_max_row: String = String::new();
            let result = std::io::stdin().read_line(&mut input_max_row);
            if let Err(err) = result {
                return Err(MyError::IOError(err));
            }
            input_max_row = input_max_row.trim().to_string();

            let result = input_max_row.parse::<u32>();
            let input_max_row = match result {
                Ok(duration) => duration,
                Err(err) => return Err(MyError::ParseIntError(err)),
            };

            Ok(Quality::Fragile {
                expiration_date: input_expiration_date,
                row: input_max_row,
            })
        }
        "1" => {
            println!("Insert item size");

            let mut input_size: String = String::new();
            let result = std::io::stdin().read_line(&mut input_size);
            if let Err(err) = result {
                return Err(MyError::IOError(err));
            }
            input_size = input_size.trim().to_string();

            let result = input_size.parse::<u32>();
            let input_size = match result {
                Ok(duration) => duration,
                Err(err) => return Err(MyError::ParseIntError(err)),
            };

            Ok(Quality::Oversized {
                continuous_zones: input_size,
            })
        }
        "2" => Ok(Quality::Normal),
        _ => Err(MyError::WrongOption(input_quality.trim().to_string())),
    };

    let item = Item {
        id: input_id,
        name: input_name.trim().to_string(),
        quantity: input_quantity,
        quality: product_quality?,
    };
    Ok(item)
}

fn main() {

    // allocation = Round robin  (didn't have enough time to impl the other,
    // but at least I used traits so it could be done without breaking the rest
    // of the code...)
    let mut supermarket = Placement::new();

    // setup filters
    let filter1 = AvoidTooLarge { cutoff: 3 };  // oversized items must not be larger than cutoff
    let filter2 = AvoidTooFragile { cutoff: 2 }; // fragile items must at least have this much flexibility
    let mut filters = Vec::<Box<dyn Filter>>::new();
    filters.push(Box::from(filter1));
    filters.push(Box::from(filter2));

    supermarket.configure_filters(filters);


    println!("Booting app....");

    let item0 = Item {
        id: 1,
        name: "Item1".to_string(),
        quantity: 1,
        quality: Quality::Normal,
    };
    let item1 = Item {
        id: 2,
        name: "Item2".to_string(),
        quantity: 1,
        quality: Quality::Oversized {
            continuous_zones: 3,
        },
    };
    let item2 = Item {
        id: 3,
        name: "Item3".to_string(),
        quantity: 1,
        quality: Quality::Normal,
    };
    let item3 = Item {
        id: 4,
        name: "Item4".to_string(),
        quantity: 1,
        quality: Quality::Oversized {
            continuous_zones: 3,
        },
    };

    let item4 = Item {
        id: 5,
        name: "Item5".to_string(),
        quantity: 1,
        quality: Quality::Fragile {
            expiration_date: [01, 01, 1999],
            row: 2,
        },
    };

    supermarket.add_item(item0.clone()).unwrap();
    //println!("{}", supermarket);

    supermarket.add_item(item1.clone()).unwrap();
    //println!("{}", supermarket);

    supermarket.add_item(item2.clone()).unwrap();
    //println!("{}", supermarket);

    supermarket.add_item(item3.clone()).unwrap();

    supermarket.add_item(item4.clone()).unwrap();


    println!("Added some example stuff inside the market....");

    println!("{}", supermarket);



    /*
    let filter1 = AvoidTooLarge { cutoff: 3 };
    let filter2 = AvoidTooFragile { cutoff: 2 };
    let mut filters = Vec::<Box<dyn Filter>>::new();
    filters.push(Box::from(filter1));
    filters.push(Box::from(filter2));
    test.configure_filters(filters);

    test.add_item(item0.clone());
    test.add_item(item1.clone());
    test.add_item(item2.clone());

    // test.remove_item(2);

    test.add_item(item3.clone());
    test.add_item(item0.clone());
    test.add_item(item4.clone());

    // println!("{}", test);

    let alphy = test.alphabetical();
    println!("{:#?}", alphy);
    println!("Check expiration");
    println!("{:#?}", test.check_expired_products([02,02,1999]))

     */
    loop {
        println!(
            "Don't steal\n\
        0: add new item\n\
        1: remove item \n\
        2: list alphabetically \n\
        3: get by ID \n\
        4: get by Name \n\
        5: list positions by ID \n\
        6: list expired :( \n\
        7: quit"
        );

        let mut option: String = String::new();
        let _ = std::io::stdin().read_line(&mut option); // error is caught in match below

        match option.trim() {
            "0" => {
                let new_item = match ask_new_product() {
                    Ok(item) => item,
                    Err(err) => {
                        println!("{}", err);
                        continue;
                    }
                };
                if let Err(err) = supermarket.add_item(new_item) {
                    println!("{}", err);
                    continue;
                }
            }
            "1" => {
                let result = ask_id();
                let result = match result {
                    Ok(item_id) => supermarket.remove_item(item_id),
                    Err(err) => {
                        println!("{}", err);
                        continue;
                    }
                };
                if let Err(err) = result {
                    println!("{}", err);
                    continue;
                }
            }
            "2" => {
                let list = supermarket.alphabetical();
                for x in list {
                    println!("{}", x);
                }
            }
            "3" => {
                let result = ask_id();
                let maybe_item = match result {
                    Ok(item_id) => supermarket.id_search(item_id),
                    Err(err) => {
                        println!("{}", err);
                        continue;
                    }
                };
                match maybe_item {
                    Some(item) => println!("{}", item),
                    None => {
                        println!("No items correspond to provided ID");
                    }
                }
            }
            "4" => {
                let result = ask_name();
                let maybe_item = match result {
                    Ok(item_name) => supermarket.name_search(item_name),
                    Err(err) => {
                        println!("{}", err);
                        continue;
                    }
                };
                match maybe_item {
                    Some(item) => println!("{}", item),
                    None => {
                        println!("No items correspond to provided Name");
                    }
                }
            }
            "5" => {
                let result = ask_id();
                let maybe_positions = match result {
                    Ok(item_id) => supermarket.position_search(item_id),
                    Err(err) => {
                        println!("{}", err);
                        continue;
                    }
                };
                match maybe_positions {
                    Some(positions) => {
                        for pos in positions {
                            println!("{}", pos)
                        }
                    }
                    None => {
                        println!("No items correspond to provided ID");
                    }
                }
            }
            "6" => {
                let result = ask_expiration_date();
                match result {
                    Ok(expiration_date) => {
                        let maybe_list = supermarket.check_expired_products(expiration_date);
                        match maybe_list {
                            Some(list) => {
                                for x in list {
                                    println!("{}", x);
                                }
                            }
                            None => {
                                println!("No expired items!! :D");
                            }
                        }
                    }
                    Err(err) => {
                        println!("{}", err);
                        continue;
                    }
                }
            }
            "7" => break,
            _ => {
                let err: Result<(), MyError> = Err(MyError::WrongOption(option.trim().to_string()));
                println!("{:?}", err.unwrap_err());
            }
        };
    }
}

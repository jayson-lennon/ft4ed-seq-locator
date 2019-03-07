#[macro_use]
extern crate stdweb;

use std::cell::RefCell;
use std::fmt;
use std::num::ParseIntError;
use std::rc::Rc;
use stdweb::traits::*;
use stdweb::unstable::TryInto;
use stdweb::web::event::{ChangeEvent, InputEvent, MouseDownEvent, MouseOverEvent, TouchMove};
use stdweb::web::html_element::InputElement;
use stdweb::web::{document, Element};

trait ElementQuery {
    fn query(&self, query: &str) -> Result<Element, AppError>;
}

impl ElementQuery for Element {
    fn query(&self, query: &str) -> Result<Element, AppError> {
        self.query_selector(query)
            .unwrap()
            .ok_or(AppError::MissingElement(query.to_owned()))
    }
}

fn div() -> Element {
    document().create_element("div").unwrap()
}

fn element_from_point(x: f64, y: f64) -> Option<Element> {
    let el = js! {
        var el = document.elementFromPoint(@{x}, @{y});
        return el;
    };
    el.try_into().ok()
}

#[derive(Debug)]
enum AppError {
    MissingElement(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AppError::MissingElement(name) => write!(f, "missing element: {}", name),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
enum RackError {
    OutOfRange(usize, usize),
    NotANumber,
}

impl fmt::Display for RackError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RackError::OutOfRange(min, max) => {
                write!(f, "Sequence must be between {} and {}.", min, max)
            }
            RackError::NotANumber => write!(f, "Sequence must be a positive integer."),
        }
    }
}

fn parse_usize(value: &str) -> Result<usize, ParseIntError> {
    match usize::from_str_radix(value, 10) {
        Ok(n) => Ok(n),
        Err(e) => {
            let s = format!("invalid number: {:?} - {}", value, e);
            Err(e)
        }
    }
}

// InsertPosition taken from webapi module in stdweb
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum InsertPosition {
    BeforeBegin,
    AfterBegin,
    BeforeEnd,
    AfterEnd,
}

impl InsertPosition {
    fn as_str(&self) -> &str {
        match *self {
            InsertPosition::BeforeBegin => "beforebegin",
            InsertPosition::AfterBegin => "afterbegin",
            InsertPosition::BeforeEnd => "beforeend",
            InsertPosition::AfterEnd => "afterend",
        }
    }
}

fn insert_adjacent_element(target: &Element, position: InsertPosition, el: &Element) {
    js! { @(no_return)
        @{target.as_ref()}.insertAdjacentElement(@{position.as_str()}, @{el.as_ref()});
    };
}

/// T4ED racks are stored in this fashion:
/// 80 32 16
/// .. .. ..
/// .  8  4
/// .  7  3
/// .  6  2
/// .  5  1
struct T4edRack {
    locations: Vec<Element>,
    dirty_locations: Vec<usize>,
    parent: Element,
    rack_indicator: Element,
}

impl T4edRack {
    pub fn new(parent: &Element) -> Self {
        let columns = parent.query(".scan-loc__column-container").unwrap();

        let rack_indicator = parent.query(".scan-loc__pagination_number").unwrap();

        let locations = {
            let mut locations = vec![];

            let mut columns_created = 0;
            while columns_created < 5 {
                let column = div();
                column.class_list().add("scan-loc__column");
                insert_adjacent_element(&columns, InsertPosition::AfterBegin, &column);

                let mut cells = vec![];

                for i in 0..16 {
                    let cell = div();
                    cell.class_list().add("scan-loc__cell");
                    insert_adjacent_element(&column, InsertPosition::AfterBegin, &cell);

                    let sequence = locations.len() + 1 + i;
                    cell.set_attribute("data-seq", &format!("{}", sequence));
                    cells.push(cell);
                }

                locations.append(&mut cells);
                columns_created += 1;
            }

            locations
        };

        T4edRack {
            parent: parent.clone(),
            locations,
            dirty_locations: vec![],
            rack_indicator,
        }
    }

    fn set_rack_number(&mut self, num: usize) {
        self.rack_indicator.set_text_content(&format!("{}", num));
    }

    fn clear_rack_number(&mut self) {
        self.rack_indicator.set_text_content("");
    }

    pub fn rack_number(&self) -> usize {
        parse_usize(
            &self
                .rack_indicator
                .text_content()
                .unwrap_or_else(|| "1".to_owned()),
        )
        .unwrap()
    }

    pub fn highlight_location(&mut self, seq: usize) -> bool {
        if seq > 160 {
            return false;
        }

        let seq = {
            if seq > 80 {
                seq - 80
            } else {
                seq
            }
        };

        self.deactivate_all();
        let seq = seq - 1;
        if seq < self.locations.len() {
            self.locations[seq]
                .class_list()
                .add("scan-loc__cell--selected");
            self.dirty_locations.push(seq);
            true
        } else {
            false
        }
    }

    pub fn deactivate_all(&mut self) {
        for el in self.dirty_locations.iter() {
            self.locations[*el]
                .class_list()
                .remove("scan-loc__cell--selected");
        }
        self.dirty_locations.clear();
    }
}

struct ErrorDisplay {
    container: Element,
    errors: Vec<(RackError, Element)>,
}

impl ErrorDisplay {
    pub fn new(container: Element) -> Self {
        ErrorDisplay {
            container,
            errors: vec![],
        }
    }

    pub fn add_error(&mut self, error: RackError) {
        if !self
            .errors
            .iter()
            .any(|e| std::mem::discriminant(&e.0) == std::mem::discriminant(&error))
        {
            let el = document().create_element("div").unwrap();
            el.set_text_content(&format!("{}", error));
            self.container.append_child(&el);
            self.errors.push((error, el));
        }
    }

    pub fn clear_error(&mut self, error: RackError) {
        if let Some(i) = self
            .errors
            .iter()
            .position(|e| std::mem::discriminant(&e.0) == std::mem::discriminant(&error))
        {
            let (_, el) = self.errors.remove(i);
            el.remove();
        }
    }

    pub fn clear_all(&mut self) {
        for i in 0..self.errors.len() {
            let (_, el) = self.errors.remove(i);
            el.remove();
        }
    }
}

fn get_element_by_id(id: &str) -> Result<Element, AppError> {
    document()
        .get_element_by_id(id)
        .ok_or(AppError::MissingElement(id.to_owned()))
}

fn document_query_selector(query: &str) -> Result<Element, AppError> {
    document()
        .query_selector(query)
        .unwrap()
        .ok_or(AppError::MissingElement(query.to_owned()))
}

fn handle_input_change(rack: &mut T4edRack, errors: &mut ErrorDisplay, value: &str) {
    match parse_usize(value) {
        Ok(seq) => {
            errors.clear_error(RackError::NotANumber);
            if !rack.highlight_location(seq) {
                errors.add_error(RackError::OutOfRange(1, 160));
                rack.clear_rack_number();
                rack.deactivate_all();
            } else {
                errors.clear_error(RackError::OutOfRange(0, 0));
                if seq > 80 {
                    rack.set_rack_number(2);
                } else {
                    rack.set_rack_number(1);
                }
            }
        }
        Err(e) => {
            rack.clear_rack_number();
            errors.clear_error(RackError::OutOfRange(0, 0));
            rack.deactivate_all();
            if value == "" {
                errors.clear_error(RackError::NotANumber);
            } else {
                errors.add_error(RackError::NotANumber);
            }
        }
    }
}

fn run() -> Result<(), AppError> {
    let mount_point = document_query_selector(".scan-loc")?;
    let mut app = Rc::new(RefCell::new(T4edRack::new(&mount_point)));

    let input_box = mount_point.query(".scan-loc__location-picker")?;
    let input_error_display = mount_point.query(".scan-loc__input-error")?;
    let mut errors = Rc::new(RefCell::new(ErrorDisplay::new(input_error_display)));

    {
        let input: InputElement = input_box.clone().try_into().unwrap();
        let mut app = app.borrow_mut();
        let mut errors = errors.borrow_mut();
        handle_input_change(&mut app, &mut errors, &input.raw_value());
    }

    {
        let app = app.clone();
        let errors = errors.clone();
        input_box.add_event_listener(move |ev: InputEvent| {
            let target: InputElement = ev.target().unwrap().try_into().unwrap();
            let raw_value = target.raw_value();
            let mut app = app.borrow_mut();
            let mut errors = errors.borrow_mut();
            handle_input_change(&mut app, &mut errors, &raw_value);
        });
    }

    {
        let app = app.clone();
        let errors = errors.clone();
        input_box.add_event_listener(move |ev: ChangeEvent| {
            let target: InputElement = ev.target().unwrap().try_into().unwrap();
            let raw_value = target.raw_value();
            let mut app = app.borrow_mut();
            let mut errors = errors.borrow_mut();
            handle_input_change(&mut app, &mut errors, &raw_value);
        });
    }

    let cells = mount_point.query_selector_all(".scan-loc__cell").unwrap();
    for cell in cells.iter() {
        {
            let app = app.clone();
            let errors = errors.clone();
            let input: InputElement = input_box.clone().try_into().unwrap();
            cell.add_event_listener(move |ev: TouchMove| {
                let touch = &ev.touches()[0];
                let (x, y) = (touch.client_x(), touch.client_y());
                let target: Element = match element_from_point(x, y) {
                    Some(el) => el,
                    None => return,
                };
                let raw_value = match target.get_attribute("data-seq") {
                    Some(v) => v,
                    None => return,
                };
                let mut app = app.borrow_mut();
                let mut errors = errors.borrow_mut();
                input.set_raw_value(&raw_value);
                handle_input_change(&mut app, &mut errors, &raw_value);
            });
        }
        {
            let app = app.clone();
            let errors = errors.clone();
            let input: InputElement = input_box.clone().try_into().unwrap();
            cell.add_event_listener(move |ev: MouseOverEvent| {
                let target: Element = ev.target().unwrap().try_into().unwrap();
                let raw_value = target.get_attribute("data-seq").unwrap();
                input.set_raw_value(&raw_value);
                let mut app = app.borrow_mut();
                let mut errors = errors.borrow_mut();
                handle_input_change(&mut app, &mut errors, &raw_value);
            });
        }

        {
            let app = app.clone();
            let errors = errors.clone();
            let input: InputElement = input_box.clone().try_into().unwrap();
            cell.add_event_listener(move |ev: MouseDownEvent| {
                let target: Element = ev.target().unwrap().try_into().unwrap();
                let raw_value = target.get_attribute("data-seq").unwrap();
                input.set_raw_value(&raw_value);
                let mut app = app.borrow_mut();
                let mut errors = errors.borrow_mut();
                handle_input_change(&mut app, &mut errors, &raw_value);
            });
        }
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        let msg = format!("{}", e);
        js! { console.log( @{msg} ) };
    }
}

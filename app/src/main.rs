#![allow(clippy::pedantic)]

#[macro_use]
extern crate stdweb;

use std::cell::RefCell;
use std::fmt;
use std::num::ParseIntError;
use std::rc::Rc;
use stdweb::traits::*;
use stdweb::unstable::TryInto;
use stdweb::web::event::{InputEvent, MouseDownEvent, MouseOverEvent, TouchMove};
use stdweb::web::html_element::InputElement;
use stdweb::web::window;
use stdweb::web::{document, Element, HtmlElement};

/// Convenience trait to query child elements.
trait ElementQuery {
    fn query(&self, query: &str) -> Result<Element, AppError>;
}

impl ElementQuery for Element {
    /// Input query should always be well-formed.
    fn query(&self, query: &str) -> Result<Element, AppError> {
        self.query_selector(query)
            .expect("Malformed query selector")
            .ok_or_else(|| AppError::MissingElement(query.to_owned()))
    }
}

/// Shorthand to create a div.
fn div() -> Element {
    document().create_element("div").unwrap()
}

/// Get the element located under the position (`x`, `y`). Coordinates are relative to the
/// viewport.
// TODO: Get this implemented in stdweb.
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
pub enum RackError {
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
    usize::from_str_radix(value, 10)
}

// InsertPosition taken from webapi module in stdweb
#[allow(dead_code)]
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
/// ```
/// 80 32 16
/// .. .. ..
/// .  8  4
/// .  7  3
/// .  6  2
/// .  5  1
/// ```
pub struct T4edRack {
    locations: Vec<Element>,
    dirty_locations: Vec<usize>,
    rack_indicator: Element,
    columns: Element,
    parent: Element,
}

impl T4edRack {
    #[allow(unused_must_use)]
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
            columns,
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

    #[allow(unused_must_use)]
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

    #[allow(unused_must_use)]
    pub fn deactivate_all(&mut self) {
        for el in self.dirty_locations.iter() {
            self.locations[*el]
                .class_list()
                .remove("scan-loc__cell--selected");
        }
        self.dirty_locations.clear();
    }

    pub fn parent(&self) -> &Element {
        &self.parent
    }

    pub fn columns(&self) -> &Element {
        &self.columns
    }
}

macro_rules! eq_variant {
    ($e1:expr, $e2:expr) => {
        std::mem::discriminant($e1) == std::mem::discriminant($e2)
    };
}

/// Possible errors that may occur when processing a scan location.
pub struct ErrorDisplay {
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

    /// Add a new error to the display.
    pub fn add_error(&mut self, error: RackError) {
        if !self.errors.iter().any(|e| eq_variant!(&e.0, &error)) {
            let el = document().create_element("div").unwrap();
            el.class_list().add("scan-loc__error");
            el.set_text_content(&format!("{}", error));
            self.container.append_child(&el);
            self.errors.push((error, el));
        }
    }

    /// Clear a specific error.
    pub fn clear_error(&mut self, error: RackError) {
        if let Some(i) = self.errors.iter().position(|e| eq_variant!(&e.0, &error)) {
            let (_, el) = self.errors.remove(i);
            el.remove();
        }
    }

    /// Clear all errors
    #[allow(dead_code)]
    pub fn clear_all(&mut self) {
        self.errors.clear();
    }
}

fn document_query_selector(query: &str) -> Result<Element, AppError> {
    document()
        .query_selector(query)
        .unwrap()
        .ok_or_else(|| AppError::MissingElement(query.to_owned()))
}

fn set_max_height(el: &Element, max_px_height: f64) {
    el.set_attribute("style", &format!("max-height: {}px", max_px_height));
    console!(log, "set max height = {}", max_px_height);
}

fn scroll_to_element(el: &Element) {
    let el: HtmlElement = el.clone().try_into().unwrap();
    let rect = el.get_bounding_client_rect();
    let scroll_top = window().page_y_offset();
    let y = rect.get_top();
    let target = y + scroll_top;

    js! { @(no_return)
        window.scrollTo(0, @{target});
    }
    console!(log, "scroll to={}", target);
}

fn handle_input_change(rack: &mut T4edRack, errors: &mut ErrorDisplay, value: &str, scroll: bool) {
    match parse_usize(value) {
        Ok(seq) => {
            let height = window().inner_height();
            let container = document_query_selector(".scan-loc__location-display").unwrap();
            set_max_height(&rack.columns, height as f64);
            if scroll {
                scroll_to_element(&rack.columns);
            }

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
        Err(_) => {
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

/// Contains event listeners for individual cells.
mod cell_events {
    use super::*;

    pub fn bind_touch(
        cell: &Element,
        app: Rc<RefCell<T4edRack>>,
        errors: Rc<RefCell<ErrorDisplay>>,
        location_picker: &Element,
    ) {
        let app = app.clone();
        let errors = errors.clone();
        let input: InputElement = location_picker.clone().try_into().unwrap();
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
            handle_input_change(&mut app, &mut errors, &raw_value, false);
        });
    }

    pub fn bind_mouse_over(
        cell: &Element,
        app: Rc<RefCell<T4edRack>>,
        errors: Rc<RefCell<ErrorDisplay>>,
        location_picker: &Element,
    ) {
        let app = app.clone();
        let errors = errors.clone();
        let input: InputElement = location_picker.clone().try_into().unwrap();
        cell.add_event_listener(move |ev: MouseOverEvent| {
            let target: Element = ev.target().unwrap().try_into().unwrap();
            let raw_value = target.get_attribute("data-seq").unwrap();
            input.set_raw_value(&raw_value);
            let (mut app, mut errors) = (app.borrow_mut(), errors.borrow_mut());
            handle_input_change(&mut app, &mut errors, &raw_value, false);
        });
    }

    pub fn bind_mouse_down(
        cell: &Element,
        app: Rc<RefCell<T4edRack>>,
        errors: Rc<RefCell<ErrorDisplay>>,
        location_picker: &Element,
    ) {
        let app = app.clone();
        let errors = errors.clone();
        let input: InputElement = location_picker.clone().try_into().unwrap();
        cell.add_event_listener(move |ev: MouseDownEvent| {
            let target: Element = ev.target().unwrap().try_into().unwrap();
            let raw_value = target.get_attribute("data-seq").unwrap();
            input.set_raw_value(&raw_value);
            let (mut app, mut errors) = (app.borrow_mut(), errors.borrow_mut());
            handle_input_change(&mut app, &mut errors, &raw_value, false);
        });
    }
}

fn run() -> Result<(), AppError> {
    let mount_point = document_query_selector(".scan-loc")?;
    let app = Rc::new(RefCell::new(T4edRack::new(&mount_point)));

    let location_picker = mount_point.query(".scan-loc__location-picker")?;
    let input_error_display = mount_point.query(".scan-loc__errors")?;
    let errors = Rc::new(RefCell::new(ErrorDisplay::new(input_error_display)));

    // Reset when page load. This is needed in case the user refreshes the page and there is a
    // value remaining in the input box.
    {
        let input: InputElement = location_picker.clone().try_into().unwrap();
        let mut app = app.borrow_mut();
        let mut errors = errors.borrow_mut();
        handle_input_change(&mut app, &mut errors, &input.raw_value(), true);
    }

    // Bind to InputEvent. This will handle manual user input on the input box.
    {
        let app = app.clone();
        let errors = errors.clone();
        location_picker.add_event_listener(move |ev: InputEvent| {
            let target: InputElement = ev.target().unwrap().try_into().unwrap();
            let raw_value = target.raw_value();
            let mut app = app.borrow_mut();
            let mut errors = errors.borrow_mut();
            handle_input_change(&mut app, &mut errors, &raw_value, true);
        });
    }

    // Here we bind each cell to allow touch and mouse interactivity.
    let cells = mount_point.query_selector_all(".scan-loc__cell").unwrap();
    for cell in cells.iter() {
        let cell: Element = cell.try_into().unwrap();
        cell_events::bind_touch(&cell, app.clone(), errors.clone(), &location_picker);
        cell_events::bind_mouse_over(&cell, app.clone(), errors.clone(), &location_picker);
        cell_events::bind_mouse_down(&cell, app.clone(), errors.clone(), &location_picker);
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        let msg = format!("{}", e);
        js! { console.log( @{msg} ) };
    }
}

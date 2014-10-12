//! The package provides a parser for the
//! [TGFF](http://ziyang.eecs.umich.edu/~dickrp/tgff/) (Task Graphs For Free)
//! format, which is a format for storing task graphs and accompanying data
//! used in scheduling and allocation research.

#![feature(macro_rules, if_let)]

use std::collections::HashMap;
use std::iter::Peekable;
use std::str::CharOffsets;

pub use content::Content;
pub use content::{Graph, Task, Arc, Deadline};
pub use content::{Table, Column};

mod content;

static READ_CAPACITY: uint = 20;

pub type Result<T> = std::result::Result<T, Error>;

pub struct Error {
    line: uint,
    message: String,
}

impl std::fmt::Show for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "{} on line {}", self.message, self.line)
    }
}

pub struct Parser<'a> {
    line: uint,
    cursor: Peekable<(uint, char), CharOffsets<'a>>,
    content: Content,
}

macro_rules! raise(
    ($parser:expr, $($arg:tt)*) => (
        return Err(Error { line: $parser.line, message: format!($($arg)*) });
    );
)

macro_rules! some(
    ($parser:expr, $result:expr, $($arg:tt)*) => (
        match $result {
            Some(result) => result,
            None => raise!($parser, $($arg)*),
        }
    );
)

impl<'a> Parser<'a> {
    /// Create a new `Parser` for processing the content of a TGFF file
    /// generated by the `tgff` command-line utility and given in `input`.
    pub fn new(input: &'a str) -> Parser<'a> {
        Parser {
            line: 1,
            cursor: input.char_indices().peekable(),
            content: Content::new(),
        }
    }

    /// Perform parsing of the data passed to `new`.
    pub fn process<'a>(&'a mut self) -> Result<&'a Content> {
        loop {
            match self.peek() {
                Some('@') => try!(self.process_at()),
                Some(_) => raise!(self, "found an unknown statement"),
                None => return Ok(&self.content),
            }
        }
    }

    fn process_at(&mut self) -> Result<()> {
        self.next(); // @

        let name = try!(self.get_token());
        let number = try!(self.get_natural());

        if let Some('{') = self.peek() {
            self.process_block(name, number)
        } else {
            self.content.set_attribute(name, number);
            Ok(())
        }
    }

    fn process_block(&mut self, name: String, id: uint) -> Result<()> {
        self.next(); // {
        self.skip_void();

        if let Some('#') = self.peek() {
            try!(self.process_table(name, id));
        } else {
            try!(self.process_graph(name, id));
        }

        if let Some('}') = self.peek() {
            self.next();
            return Ok(());
        }

        raise!(self, "cannot find the end of a block");
    }

    fn process_graph(&mut self, name: String, id: uint) -> Result<()> {
        let mut graph = Graph::new(name, id);

        loop {
            match self.read_token() {
                Some(ref token) => match token.as_slice() {
                    "TASK" => {
                        let id = try!(self.get_id());
                        try!(self.skip_str("TYPE"));
                        let kind = try!(self.get_natural());

                        graph.add_task(Task::new(id, kind));
                    },
                    "ARC" => {
                        let id = try!(self.get_id());
                        try!(self.skip_str("FROM"));
                        let from = try!(self.get_id());
                        try!(self.skip_str("TO"));
                        let to = try!(self.get_id());
                        try!(self.skip_str("TYPE"));
                        let kind = try!(self.get_natural());

                        graph.add_arc(Arc::new(id, from, to, kind));
                    },
                    "HARD_DEADLINE" => {
                        let id = try!(self.get_id());
                        try!(self.skip_str("ON"));
                        let on = try!(self.get_id());
                        try!(self.skip_str("AT"));
                        let at = try!(self.get_natural());

                        graph.add_deadline(Deadline::new(id, on, at));
                    },
                    _ => {
                        let value = try!(self.get_natural());

                        graph.set_attribute(token.clone(), value);
                    },
                },
                None => break,
            }
        }

        self.content.add_graph(graph);
        Ok(())
    }

    fn process_table(&mut self, name: String, id: uint) -> Result<()> {
        let mut table = Table::new(name, id);

        self.content.add_table(table);
        Ok(())
    }

    fn skip(&mut self, accept: |uint, char| -> bool) -> uint {
        let mut count = 0;

        loop {
            match self.peek() {
                Some(c) => {
                    if !accept(count, c) { break; }
                    self.next();
                    count += 1;
                },
                None => break,
            }
        }

        count
    }

    #[inline]
    fn skip_void(&mut self) {
        self.skip(|_, c| c == ' ' || c == '\t' || c == '\n');
    }

    fn skip_str(&mut self, chars: &str) -> Result<()> {
        let len = chars.len();
        if self.skip(|i, c| i < len && c == chars.char_at(i)) != len {
            raise!(self, "expected `{}`", chars);
        }
        self.skip_void();
        Ok(())
    }

    fn read(&mut self, accept: |uint, char| -> bool) -> Option<String> {
        let mut result = std::string::String::with_capacity(READ_CAPACITY);
        let mut count = 0;

        loop {
            match self.peek() {
                Some(c) => {
                    if !accept(count, c) { break; }
                    result.push(c);
                    self.next();
                    count += 1;
                },
                None => break,
            }
        }

        if count == 0 {
            None
        } else {
            Some(result)
        }
    }

    fn read_token(&mut self) -> Option<String> {
        let result = self.read(|i, c| {
            match c {
                'A'...'Z' | 'a'...'z' if i == 0 => true,
                'A'...'Z' | 'a'...'z' | '_' | '0'...'9' if i > 0 => true,
                _ => false,
            }
        });
        self.skip_void();
        result
    }

    fn read_natural(&mut self) -> Option<uint> {
        let result = match self.read(|_, c| c >= '0' && c <= '9') {
            Some(ref number) => std::num::from_str_radix(number.as_slice(), 10),
            None => None,
        };
        self.skip_void();
        result
    }

    fn read_id(&mut self) -> Option<uint> {
        match self.read_token() {
            Some(ref token) => match token.as_slice().split('_').nth(1) {
                Some(id) => std::num::from_str_radix(id, 10),
                None => None,
            },
            None => None,
        }
    }

    fn get_token(&mut self) -> Result<String> {
        match self.read_token() {
            Some(token) => Ok(token),
            None => raise!(self, "expected a token"),
        }
    }

    fn get_natural(&mut self) -> Result<uint> {
        match self.read_natural() {
            Some(number) => Ok(number),
            None => raise!(self, "expected a natural number"),
        }
    }

    fn get_id(&mut self) -> Result<uint> {
        match self.read_id() {
            Some(id) => Ok(id),
            None => raise!(self, "expected an id"),
        }
    }

    #[inline]
    fn peek(&mut self) -> Option<char> {
        match self.cursor.peek() {
            Some(&(_, c)) => Some(c),
            None => None,
        }
    }
}

impl<'a> std::iter::Iterator<char> for Parser<'a> {
    fn next(&mut self) -> Option<char> {
        match self.cursor.next() {
            Some((_, '\n')) => {
                self.line += 1;
                Some('\n')
            },
            Some((_, c)) => Some(c),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    macro_rules! assert_ok(
        ($result: expr) => (
            if let Err(err) = $result {
                assert!(false, "{}", err);
            }
        );
    )

    macro_rules! assert_error(
        ($result: expr) => (
            if let Ok(_) = $result {
                assert!(false, "expected an error");
            }
        );
    )

    macro_rules! parser(
        ($input:expr) => (super::Parser::new($input));
    )

    #[test]
    fn process_at() {
        assert_ok!(parser!("@abc 12").process_at());
        assert_error!(parser!("@ ").process_at());
        assert_error!(parser!("@abc").process_at());
    }

    #[test]
    fn process_block() {
        assert_ok!(parser!("{}").process_block(String::from_str("life"), 42));
    }

    #[test]
    fn process_graph() {
        let mut parser = parser!("TASK t0_0\tTYPE 2   ");
        parser.process_graph(String::new(), 0);
        {
            let ref task = parser.content.graphs[0].tasks[0];
            assert_eq!(task.id, 0);
            assert_eq!(task.kind, 2);
        }

        parser = parser!("ARC a0_42 \tFROM t0_0  TO  t0_1 TYPE 35   ");
        parser.process_graph(String::new(), 0);
        {
            let ref arc = parser.content.graphs[0].arcs[0];
            assert_eq!(arc.id, 42);
            assert_eq!(arc.from, 0);
            assert_eq!(arc.to, 1);
            assert_eq!(arc.kind, 35);
        }

        parser = parser!("HARD_DEADLINE d0_9 ON t0_12 AT 1000   ");
        parser.process_graph(String::new(), 0);
        {
            let ref deadline = parser.content.graphs[0].deadlines[0];
            assert_eq!(deadline.id, 9);
            assert_eq!(deadline.on, 12);
            assert_eq!(deadline.at, 1000);
        }
    }

    #[test]
    fn process_table() {
    }

    #[test]
    fn skip_void() {
        let mut parser = parser!("  \t  abc");
        parser.skip_void();
        assert_eq!(parser.next().unwrap(), 'a');
    }

    #[test]
    fn get_token() {
        macro_rules! test(
            ($input:expr, $output:expr) => (
                assert_eq!(parser!($input).get_token().unwrap(),
                           String::from_str($output));
            );
        )

        test!("AZ xyz", "AZ");
        test!("az xyz", "az");
        test!("AZ_az_09 xyz", "AZ_az_09");
    }

    #[test]
    fn get_natural() {
        assert_eq!(parser!("09").get_natural().unwrap(), 9);
    }

    #[test]
    fn get_id() {
        assert_eq!(parser!("t0_42").get_id().unwrap(), 42);
    }
}

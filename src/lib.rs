//! The package provides a parser for the
//! [TGFF](http://ziyang.eecs.umich.edu/~dickrp/tgff/) (Task Graphs For Free)
//! format, which is a format for storing task graphs and accompanying data
//! used in scheduling and allocation research.

#![feature(macro_rules, if_let)]

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
        try!(self.skip_char('@'));

        let name = try!(self.get_token());
        let number = try!(self.get_natural());

        if let Some('{') = self.peek() {
            self.process_block(name, number)
        } else {
            self.content.attributes.insert(name, number);
            Ok(())
        }
    }

    fn process_block(&mut self, name: String, id: uint) -> Result<()> {
        try!(self.skip_char('{'));
        if let Some('#') = self.peek() {
            try!(self.process_table(name, id));
        } else {
            try!(self.process_graph(name, id));
        }
        try!(self.skip_char('}'));
        Ok(())
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
                        graph.tasks.push(Task::new(id, kind));
                    },
                    "ARC" => {
                        let id = try!(self.get_id());
                        try!(self.skip_str("FROM"));
                        let from = try!(self.get_id());
                        try!(self.skip_str("TO"));
                        let to = try!(self.get_id());
                        try!(self.skip_str("TYPE"));
                        let kind = try!(self.get_natural());
                        graph.arcs.push(Arc::new(id, from, to, kind));
                    },
                    "HARD_DEADLINE" => {
                        let id = try!(self.get_id());
                        try!(self.skip_str("ON"));
                        let on = try!(self.get_id());
                        try!(self.skip_str("AT"));
                        let at = try!(self.get_natural());
                        graph.deadlines.push(Deadline::new(id, on, at));
                    },
                    _ => {
                        let value = try!(self.get_natural());
                        graph.attributes.insert(token.clone(), value);
                    },
                },
                None => break,
            }
        }

        self.content.graphs.push(graph);
        Ok(())
    }

    fn process_table(&mut self, name: String, id: uint) -> Result<()> {
        let mut table = Table::new(name, id);

        try!(self.skip_char('#'));

        let mut names = vec![];
        loop {
            match self.read_token() {
                Some(token) => names.push(token),
                None => break,
            }
        }
        for name in names.into_iter() {
            table.attributes.insert(name, try!(self.get_real()));
        }

        try!(self.skip_comment());
        try!(self.skip_char('#'));

        loop {
            match self.read_token() {
                Some(name) => table.columns.push(Column::new(name)),
                None => break,
            }
        }
        let cols = table.columns.len();
        loop {
            match self.peek() {
                Some('}') | None => break,
                _ => {},
            }
            for i in range(0u, cols) {
                table.columns.get_mut(i).data.push(try!(self.get_real()));
            }
        }

        self.content.tables.push(table);
        Ok(())
    }

    #[inline]
    fn peek(&mut self) -> Option<char> {
        match self.cursor.peek() {
            Some(&(_, c)) => Some(c),
            None => None,
        }
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

    fn skip_char(&mut self, expected: char) -> Result<()> {
        match self.next() {
            Some(c) => {
                if c == expected {
                    self.skip_void();
                    return Ok(());
                }
            },
            None => {},
        }
        raise!(self, "expected `{}`", expected);
    }

    fn skip_str(&mut self, expected: &str) -> Result<()> {
        let len = expected.len();
        if self.skip(|i, c| i < len && c == expected.char_at(i)) != len {
            raise!(self, "expected `{}`", expected);
        }
        self.skip_void();
        Ok(())
    }

    #[inline]
    fn skip_void(&mut self) {
        self.skip(|_, c| c == ' ' || c == '\t' || c == '\n');
    }

    fn skip_comment(&mut self) -> Result<()> {
        if self.skip(|i, c| i == 0 && c == '#' || (i > 0) && c == '-') < 2 {
            raise!(self, "expected a comment line");
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

    fn read_id(&mut self) -> Option<uint> {
        match self.read_token() {
            Some(ref token) => match token.as_slice().split('_').nth(1) {
                Some(id) => std::num::from_str_radix(id, 10),
                None => None,
            },
            None => None,
        }
    }

    fn read_natural(&mut self) -> Option<uint> {
        let result = match self.read(|_, c| c >= '0' && c <= '9') {
            Some(ref number) => std::num::from_str_radix(number.as_slice(), 10),
            None => None,
        };
        self.skip_void();
        result
    }

    fn read_real(&mut self) -> Option<f64> {
        let result = match self.read(|_, c| {
            match c {
                '+' | '-' | '.' | '0'...'9' | 'e' | 'E' => true,
                _ => false,
            }
        }) {
            Some(ref number) => {
                use std::num::strconv;
                strconv::from_str_common(number.as_slice(), 10, true, true,
                                         false, strconv::ExpDec, false, false)
            },
            None => None,
        };
        self.skip_void();
        result
    }

    fn get_token(&mut self) -> Result<String> {
        match self.read_token() {
            Some(token) => Ok(token),
            None => raise!(self, "expected a token"),
        }
    }

    fn get_id(&mut self) -> Result<uint> {
        match self.read_id() {
            Some(id) => Ok(id),
            None => raise!(self, "expected an id"),
        }
    }

    fn get_natural(&mut self) -> Result<uint> {
        match self.read_natural() {
            Some(number) => Ok(number),
            None => raise!(self, "expected a natural number"),
        }
    }

    fn get_real(&mut self) -> Result<f64> {
        match self.read_real() {
            Some(number) => Ok(number),
            None => raise!(self, "expected a real number"),
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
        assert_ok!(parser!("{}").process_block(String::new(), 0));
    }

    #[test]
    fn process_graph() {
        let mut parser = parser!("TASK t0_0\tTYPE 2   ");
        assert_ok!(parser.process_graph(String::new(), 0));
        {
            let ref task = parser.content.graphs[0].tasks[0];
            assert_eq!(task.id, 0);
            assert_eq!(task.kind, 2);
        }

        parser = parser!("ARC a0_42 \tFROM t0_0  TO  t0_1 TYPE 35   ");
        assert_ok!(parser.process_graph(String::new(), 0));
        {
            let ref arc = parser.content.graphs[0].arcs[0];
            assert_eq!(arc.id, 42);
            assert_eq!(arc.from, 0);
            assert_eq!(arc.to, 1);
            assert_eq!(arc.kind, 35);
        }

        parser = parser!("HARD_DEADLINE d0_9 ON t0_12 AT 1000   ");
        assert_ok!(parser.process_graph(String::new(), 0));
        {
            let ref deadline = parser.content.graphs[0].deadlines[0];
            assert_eq!(deadline.id, 9);
            assert_eq!(deadline.on, 12);
            assert_eq!(deadline.at, 1000);
        }
    }

    #[test]
    fn process_table() {
        let mut parser = parser!("# foo\n 70.07\n#--\n# bar baz\n1 2 3 4 ");
        assert_ok!(parser.process_table(String::new(), 0));
        let ref table = parser.content.tables[0];
        assert_eq!(table.attributes["foo".to_string()], 70.07);
        assert_eq!(table.columns[0].name, "bar".to_string());
        assert_eq!(table.columns[1].name, "baz".to_string());
        assert_eq!(table.columns[0].data, vec![1.0, 3.0]);
        assert_eq!(table.columns[1].data, vec![2.0, 4.0]);
    }

    #[test]
    fn skip_char() {
        let mut parser = parser!("#  \t\n  abc");
        assert_ok!(parser.skip_char('#'));
        assert_eq!(parser.next().unwrap(), 'a');
    }

    #[test]
    fn skip_str() {
        let mut parser = parser!("abc  \t\n  xyz");
        assert_ok!(parser.skip_str("abc"));
        assert_eq!(parser.next().unwrap(), 'x');
    }

    #[test]
    fn skip_void() {
        let mut parser = parser!("  \t  abc");
        parser.skip_void();
        assert_eq!(parser.next().unwrap(), 'a');
    }

    #[test]
    fn skip_comment() {
        let mut parser = parser!("#--------------   \n abc");
        assert_ok!(parser.skip_comment());
        assert_eq!(parser.next().unwrap(), 'a');
    }

    #[test]
    fn get_token() {
        macro_rules! test(
            ($input:expr, $output:expr) => (
                assert_eq!(parser!($input).get_token().unwrap(),
                           String::from_str($output))
            );
        )
        test!("AZ xyz", "AZ");
        test!("az xyz", "az");
        test!("AZ_az_09 xyz", "AZ_az_09");
    }

    #[test]
    fn get_id() {
        assert_eq!(parser!("t0_42").get_id().unwrap(), 42);
    }

    #[test]
    fn get_natural() {
        assert_eq!(parser!("09").get_natural().unwrap(), 9);
    }

    #[test]
    fn get_real() {
        macro_rules! test(
            ($input:expr, $output:expr) => (
                assert_eq!(parser!($input).get_real().unwrap(), $output)
            );
        )
        test!("-1", -1.0);
        test!("0.1", 0.1);
        test!("1.2e3", 1.2e3);
        test!("1.2e+3", 1.2e3);
        test!("-1.2e-3", -1.2e-3);
    }
}

use crate::error_handling::Error::LogParserInternalErr;
use crate::error_handling::Result;
use crate::lexer::BufferedFileStream;
use crate::lexer::LexerStream;
use crate::lexer::{Lexer, Token, TokenType};
use crate::parser::SchemaConfig;
use std::fmt::Debug;
use std::rc::Rc;

pub struct LogParser {
    lexer: Lexer,
    schema_config: Rc<SchemaConfig>,
    tokens: Option<Vec<Token>>,
}

pub struct LogEvent {
    tokens: Vec<Token>,
    line_range: (usize, usize),
    has_timestamp: bool,
    schema_config: Rc<SchemaConfig>,
}

impl LogParser {
    pub fn new(schema_config: Rc<SchemaConfig>) -> Result<Self> {
        let lexer = Lexer::new(schema_config.clone())?;
        Ok((Self {
            lexer,
            schema_config,
            tokens: Some(Vec::new()),
        }))
    }

    pub fn set_input_file(&mut self, path: &str) -> Result<()> {
        self.tokens = Some(Vec::new());
        let buffered_file_stream = Box::new(BufferedFileStream::new(path)?);
        self.set_input_stream(buffered_file_stream)
    }

    pub fn set_input_stream(&mut self, input_stream: Box<dyn LexerStream>) -> Result<()> {
        self.lexer.set_input_stream(input_stream);
        Ok(())
    }

    pub fn parse_next_log_event(&mut self) -> Result<Option<LogEvent>> {
        loop {
            match self.lexer.get_next_token()? {
                Some(token) => match token.get_token_type() {
                    TokenType::Timestamp(_) => {
                        if self.tokens.is_none() {
                            self.buffer_token(token);
                            continue;
                        }
                        let log_event = self.emit_buffered_tokens_as_log_event()?;
                        self.buffer_token(token);
                        return Ok(log_event);
                    }
                    _ => self.buffer_token(token),
                },
                None => break,
            }
        }
        self.emit_buffered_tokens_as_log_event()
    }

    fn buffer_token(&mut self, token: Token) {
        if self.tokens.is_none() {
            self.tokens = Some(Vec::new());
        }
        self.tokens.as_mut().unwrap().push(token);
    }

    fn emit_buffered_tokens_as_log_event(&mut self) -> Result<Option<LogEvent>> {
        match &self.tokens {
            Some(_) => {
                let tokens = self.tokens.take().unwrap();
                LogEvent::new(self.schema_config.clone(), tokens)
            }
            None => Ok(None),
        }
    }
}

impl LogEvent {
    fn new(schema_config: Rc<SchemaConfig>, tokens: Vec<Token>) -> Result<Option<Self>> {
        if tokens.is_empty() {
            return Err(LogParserInternalErr("The given token vector is empty"));
        }
        let has_timestamp = match tokens.first().unwrap().get_token_type() {
            TokenType::Timestamp(_) => true,
            _ => false,
        };
        let line_range = (
            tokens.first().unwrap().get_line_num(),
            tokens.last().unwrap().get_line_num(),
        );
        Ok(Some(
            (Self {
                tokens,
                line_range,
                has_timestamp,
                schema_config,
            }),
        ))
    }

    pub fn get_timestamp_token(&self) -> Option<&Token> {
        match self.has_timestamp {
            true => Some(&self.tokens[0]),
            false => None,
        }
    }

    pub fn get_line_range(&self) -> (usize, usize) {
        self.line_range
    }

    pub fn get_log_message_tokens(&self) -> &[Token] {
        match self.has_timestamp {
            true => &self.tokens[1..],
            false => &self.tokens[..],
        }
    }
}

impl Debug for LogEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut result = String::new();
        match self.get_timestamp_token() {
            Some(ts_token) => result += format!("Timestamp:\n\t{:?}\n", ts_token).as_str(),
            None => result += "Timestamp:\n\tNONE\n",
        }

        let (mut curr_line_num, _) = self.get_line_range();
        result += format!("Line {}:\n", curr_line_num).as_str();
        for token in self.get_log_message_tokens() {
            if token.get_line_num() != curr_line_num {
                curr_line_num = token.get_line_num();
                result += format!("Line {}:\n", curr_line_num).as_str();
            }
            result += format!("\t{:?}\n", token).as_str();
        }

        write!(f, "{}", result)
    }
}

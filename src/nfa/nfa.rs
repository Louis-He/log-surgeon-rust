use crate::error_handling::Result;
use crate::parser::regex_parser::parser::RegexParser;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use crate::error_handling::Error::{
    NegationNotSupported, NonGreedyRepetitionNotSupported, NoneASCIICharacters,
    UnsupportedAstBracketedKind, UnsupportedAstNodeType, UnsupportedClassSetType,
    UnsupportedGroupKindType,
};
use regex_syntax::ast::{
    Alternation, Ast, ClassBracketed, ClassPerl, ClassPerlKind, ClassSet, ClassSetItem,
    ClassSetRange, ClassSetUnion, Concat, Group, GroupKind, Literal, Repetition, RepetitionKind,
    RepetitionRange,
};

const DIGIT_TRANSITION: u128 = 0x000000000000000003ff000000000000;
const SPACE_TRANSITION: u128 = 0x00000000000000000000000100003e00;
const WORD_TRANSITION: u128 = 0x07fffffe87fffffe03ff000000000000;

const EPSILON_TRANSITION: u128 = 0x0;

const DOT_TRANSITION: u128 = !EPSILON_TRANSITION;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct State(pub usize);

pub struct Transition {
    from: State,
    to: State,
    symbol_onehot_encoding: u128,
    tag: i16,
}

impl Debug for Transition {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if 0 == self.symbol_onehot_encoding {
            return write!(f, "{:?} -> {:?}, symbol: {}", self.from, self.to, "epsilon");
        }

        let mut char_vec: Vec<char> = Vec::new();
        for i in 0..128u8 {
            let mask = 1u128 << i;
            if mask & self.symbol_onehot_encoding == mask {
                char_vec.push(i as char);
            }
        }
        write!(
            f,
            "{:?} -> {:?}, symbol: {:?}",
            self.from, self.to, char_vec
        )
    }
}

impl Transition {
    pub fn convert_char_to_symbol_onehot_encoding(c: char) -> u128 {
        let mut symbol_onehot_encoding = 0;
        let c = c as u8;

        symbol_onehot_encoding |= 1 << c;

        symbol_onehot_encoding
    }

    pub fn convert_char_range_to_symbol_onehot_encoding(range: Option<(u8, u8)>) -> u128 {
        let mut symbol_onehot_encoding: u128 = 0;

        match range {
            Some((begin, end)) => {
                for c in begin..=end {
                    symbol_onehot_encoding |= 1 << c;
                }
            }
            None => {}
        }

        symbol_onehot_encoding
    }

    pub fn convert_char_vec_to_symbol_onehot_encoding(char_vec: Vec<u8>) -> u128 {
        let mut symbol_onehot_encoding: u128 = 0;
        for c in char_vec {
            symbol_onehot_encoding |= 1 << c;
        }
        symbol_onehot_encoding
    }

    pub fn new(from: State, to: State, symbol_onehot_encoding: u128, tag: i16) -> Self {
        Transition {
            from,
            to,
            symbol_onehot_encoding,
            tag,
        }
    }

    pub fn get_symbol_onehot_encoding(&self) -> u128 {
        self.symbol_onehot_encoding
    }

    pub fn get_symbol(&self) -> Vec<char> {
        let mut symbol = vec![];
        for i in 0..=127 {
            if self.symbol_onehot_encoding & (1 << i) != 0 {
                symbol.push(i as u8 as char);
            }
        }
        symbol
    }

    pub fn get_to_state(&self) -> State {
        self.to.clone()
    }
}

pub(crate) struct NFA {
    start: State,
    accept: State,
    states: Vec<State>,
    transitions: HashMap<State, Vec<Transition>>,
}

impl NFA {
    pub const START_STATE: State = State(0);
    pub const ACCEPT_STATE: State = State(1);
}

// NFA implementation for NFA construction from AST
impl NFA {
    pub fn new() -> Self {
        let states_vec = vec![NFA::START_STATE.clone(), NFA::ACCEPT_STATE.clone()];
        NFA {
            start: NFA::START_STATE,
            accept: NFA::ACCEPT_STATE,
            states: states_vec,
            transitions: HashMap::new(),
        }
    }

    pub fn add_ast_to_nfa(&mut self, ast: &Ast, start: State, end: State) -> Result<()> {
        match ast {
            Ast::Literal(literal) => self.add_literal(&**literal, start, end)?,
            Ast::Dot(dot) => self.add_dot(start, end)?,
            Ast::ClassPerl(perl) => self.add_perl(&**perl, start, end)?,
            Ast::Repetition(repetition) => self.add_repetition(&**repetition, start, end)?,
            Ast::Concat(concat) => self.add_concat(&**concat, start, end)?,
            Ast::ClassBracketed(bracketed) => self.add_bracketed(&**bracketed, start, end)?,
            Ast::Alternation(alternation) => self.add_alternation(&**alternation, start, end)?,
            Ast::Group(group) => self.add_group(&**group, start, end)?,
            _ => {
                return Err(UnsupportedAstNodeType("Ast Type not supported"));
            }
        }
        Ok(())
    }

    fn add_literal(&mut self, literal: &Literal, start: State, end: State) -> Result<()> {
        let c = get_ascii_char(literal.c)?;
        self.add_transition_from_range(start, end, Some((c, c)));
        Ok(())
    }

    fn add_dot(&mut self, start: State, end: State) -> Result<()> {
        self.add_transition(start, end, DOT_TRANSITION);
        Ok(())
    }

    fn add_perl(&mut self, perl: &ClassPerl, start: State, end: State) -> Result<()> {
        if perl.negated {
            return Err(NegationNotSupported("Negation in perl not yet supported."));
        }
        match perl.kind {
            ClassPerlKind::Digit => self.add_transition(start, end, DIGIT_TRANSITION),
            ClassPerlKind::Space => self.add_transition(start, end, SPACE_TRANSITION),
            ClassPerlKind::Word => self.add_transition(start, end, WORD_TRANSITION),
        }
        Ok(())
    }

    fn add_concat(&mut self, concat: &Concat, start: State, end: State) -> Result<()> {
        let mut curr_start = start.clone();
        for (idx, sub_ast) in concat.asts.iter().enumerate() {
            let curr_end = if concat.asts.len() - 1 == idx {
                end.clone()
            } else {
                self.new_state()
            };
            self.add_ast_to_nfa(sub_ast, curr_start.clone(), curr_end.clone())?;
            curr_start = curr_end.clone();
        }
        Ok(())
    }

    fn add_group(&mut self, group: &Group, start: State, end: State) -> Result<()> {
        match &group.kind {
            GroupKind::CaptureIndex(_) => self.add_ast_to_nfa(&group.ast, start, end)?,
            _ => return Err(UnsupportedGroupKindType),
        }
        Ok(())
    }

    fn add_alternation(
        &mut self,
        alternation: &Alternation,
        start: State,
        end: State,
    ) -> Result<()> {
        for sub_ast in alternation.asts.iter() {
            let sub_ast_start = self.new_state();
            let sub_ast_end = self.new_state();
            self.add_epsilon_transition(start.clone(), sub_ast_start.clone());
            self.add_epsilon_transition(sub_ast_end.clone(), end.clone());
            self.add_ast_to_nfa(sub_ast, sub_ast_start, sub_ast_end)?;
        }
        Ok(())
    }

    fn add_repetition(&mut self, repetition: &Repetition, start: State, end: State) -> Result<()> {
        if false == repetition.greedy {
            return Err(NonGreedyRepetitionNotSupported);
        }

        let (min, optional_max) = Self::get_repetition_range(&repetition.op.kind);
        let mut start_state = start.clone();
        let range_bound_state = self.new_state();

        if 0 == min {
            // 0 repetitions at minimum, meaning that there's an epsilon transition start -> end
            self.add_epsilon_transition(start_state.clone(), range_bound_state.clone());
        } else {
            for _ in 1..min {
                let intermediate_state = self.new_state();
                self.add_ast_to_nfa(
                    &repetition.ast,
                    start_state.clone(),
                    intermediate_state.clone(),
                )?;
                start_state = intermediate_state;
            }
            self.add_ast_to_nfa(
                &repetition.ast,
                start_state.clone(),
                range_bound_state.clone(),
            )?;
        }

        self.add_epsilon_transition(range_bound_state.clone(), end.clone());
        match optional_max {
            None => {
                self.add_ast_to_nfa(
                    &repetition.ast,
                    range_bound_state.clone(),
                    range_bound_state.clone(),
                )?;
            }
            Some(max) => {
                if min == max {
                    // Already handled in the section above
                    return Ok(());
                }
                start_state = range_bound_state.clone();
                for _ in min..max {
                    let intermediate_state = self.new_state();
                    self.add_ast_to_nfa(
                        &repetition.ast,
                        start_state.clone(),
                        intermediate_state.clone(),
                    )?;
                    self.add_epsilon_transition(intermediate_state.clone(), end.clone());
                    start_state = intermediate_state;
                }
            }
        }

        Ok(())
    }

    fn add_bracketed(
        &mut self,
        bracketed: &ClassBracketed,
        start: State,
        end: State,
    ) -> Result<()> {
        if bracketed.negated {
            return Err(NegationNotSupported(
                "Negation in bracket not yet supported",
            ));
        }
        match &bracketed.kind {
            ClassSet::Item(item) => self.add_class_set_item(item, start, end)?,
            _ => return Err(UnsupportedAstBracketedKind),
        }
        Ok(())
    }

    fn add_class_set_item(&mut self, item: &ClassSetItem, start: State, end: State) -> Result<()> {
        match item {
            ClassSetItem::Literal(literal) => self.add_literal(literal, start, end)?,
            ClassSetItem::Bracketed(bracketed) => self.add_bracketed(bracketed, start, end)?,
            ClassSetItem::Range(range) => self.add_range(range, start, end)?,
            ClassSetItem::Perl(perl) => self.add_perl(perl, start, end)?,
            ClassSetItem::Union(union) => self.add_union(union, start, end)?,
            _ => return Err(UnsupportedClassSetType),
        }
        Ok(())
    }

    fn add_range(&mut self, range: &ClassSetRange, start: State, end: State) -> Result<()> {
        self.add_transition_from_range(
            start,
            end,
            Some((get_ascii_char(range.start.c)?, get_ascii_char(range.end.c)?)),
        );
        Ok(())
    }

    fn add_union(&mut self, union: &ClassSetUnion, start: State, end: State) -> Result<()> {
        let mut curr_start = start.clone();
        for (idx, item) in union.items.iter().enumerate() {
            let curr_end = if union.items.len() - 1 == idx {
                end.clone()
            } else {
                self.new_state()
            };
            self.add_class_set_item(item, curr_start.clone(), curr_end.clone())?;
            curr_start = curr_end.clone();
        }

        Ok(())
    }

    fn get_repetition_range(kind: &RepetitionKind) -> (u32, Option<u32>) {
        match kind {
            RepetitionKind::ZeroOrOne => (0, Some(1)),
            RepetitionKind::ZeroOrMore => (0, None),
            RepetitionKind::OneOrMore => (1, None),
            RepetitionKind::Range(range) => match range {
                RepetitionRange::Exactly(num) => (*num, Some(*num)),
                RepetitionRange::AtLeast(num) => (*num, None),
                RepetitionRange::Bounded(begin, end) => (*begin, Some(*end)),
            },
        }
    }

    fn new_state(&mut self) -> State {
        self.states.push(State(self.states.len()));
        self.states.last().unwrap().clone()
    }

    fn add_transition_from_range(&mut self, from: State, to: State, range: Option<(u8, u8)>) {
        let transition = Transition {
            from: from.clone(),
            to: to.clone(),
            symbol_onehot_encoding: Transition::convert_char_range_to_symbol_onehot_encoding(range),
            tag: -1,
        };
        self.transitions
            .entry(from)
            .or_insert(vec![])
            .push(transition);
    }

    fn add_transition(&mut self, from: State, to: State, onehot: u128) {
        let transition = Transition {
            from: from.clone(),
            to: to.clone(),
            symbol_onehot_encoding: onehot,
            tag: -1,
        };
        self.transitions
            .entry(from)
            .or_insert(vec![])
            .push(transition);
    }

    fn add_epsilon_transition(&mut self, from: State, to: State) {
        self.add_transition(from, to, EPSILON_TRANSITION);
    }
}

impl Debug for NFA {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "NFA( start: {:?}, accept: {:?}, states: {:?}, transitions: {{\n",
            self.start, self.accept, self.states
        )?;

        for state in &self.states {
            if false == self.transitions.contains_key(state) {
                continue;
            }
            write!(f, "\t{:?}:\n", state)?;
            for transition in self.transitions.get(state).unwrap() {
                write!(f, "\t\t{:?}\n", transition)?;
            }
        }

        write!(f, "}} )")
    }
}

// NFA implementation for NFA to dfa conversion helper functions
impl NFA {
    pub fn epsilon_closure(&self, states: &Vec<State>) -> Vec<State> {
        let mut closure = states.clone();
        let mut stack = states.clone();

        while let Some(state) = stack.pop() {
            let transitions = self.transitions.get(&state);
            if transitions.is_none() {
                continue;
            }

            for transition in transitions.unwrap() {
                if transition.symbol_onehot_encoding == 0 {
                    let to_state = transition.to.clone();
                    if !closure.contains(&to_state) {
                        closure.push(to_state.clone());
                        stack.push(to_state);
                    }
                }
            }
        }

        closure
    }

    // Static function to get the combined state names
    pub fn get_combined_state_names(states: &Vec<State>) -> String {
        let mut names = states
            .iter()
            .map(|state| state.0.to_string())
            .collect::<Vec<String>>();
        names.sort();
        names.join(",")
    }
}

// Getter functions for NFA
impl NFA {
    pub fn get_start(&self) -> State {
        self.start.clone()
    }

    pub fn get_accept(&self) -> State {
        self.accept.clone()
    }

    pub fn get_transitions(&self) -> &HashMap<State, Vec<Transition>> {
        &self.transitions
    }

    pub fn get_transitions_from_state(&self, state: &State) -> Option<&Vec<Transition>> {
        self.transitions.get(state)
    }
}

// Helper functions
fn get_ascii_char(c: char) -> Result<u8> {
    if false == c.is_ascii() {
        return Err(NoneASCIICharacters);
    }
    Ok(c as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_char() -> Result<()> {
        let mut parser = RegexParser::new();
        let parsed_ast = parser.parse_into_ast(r"&")?;
        let mut nfa = NFA::new();
        nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

        assert!(has_transition(
            &nfa,
            NFA::START_STATE,
            NFA::ACCEPT_STATE,
            Transition::convert_char_to_symbol_onehot_encoding('&')
        ));
        Ok(())
    }

    #[test]
    fn test_dot() -> Result<()> {
        {
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r".")?;
            let mut nfa = NFA::new();
            nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

            assert!(has_transition(
                &nfa,
                NFA::START_STATE,
                NFA::ACCEPT_STATE,
                Transition::convert_char_range_to_symbol_onehot_encoding(Some((0, 127)))
            ));
        }

        {
            // Testing escaped `.`
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r"\.")?;
            let mut nfa = NFA::new();
            nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

            assert!(has_transition(
                &nfa,
                NFA::START_STATE,
                NFA::ACCEPT_STATE,
                Transition::convert_char_to_symbol_onehot_encoding('.')
            ));
        }
        Ok(())
    }

    #[test]
    fn test_perl() -> Result<()> {
        {
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r"\d")?;

            let mut nfa = NFA::new();
            nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

            let char_vec: Vec<u8> = (b'0'..=b'9').collect();
            assert!(has_transition(
                &nfa,
                NFA::START_STATE,
                NFA::ACCEPT_STATE,
                Transition::convert_char_vec_to_symbol_onehot_encoding(char_vec)
            ));
        }

        {
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r"\s")?;

            let mut nfa = NFA::new();
            nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

            let char_vec = vec![
                b' ',    // Space
                b'\t',   // Horizontal Tab
                b'\n',   // Line Feed
                b'\r',   // Carriage Return
                b'\x0B', // Vertical Tab
                b'\x0C', // Form Feed
            ];
            assert!(has_transition(
                &nfa,
                NFA::START_STATE,
                NFA::ACCEPT_STATE,
                Transition::convert_char_vec_to_symbol_onehot_encoding(char_vec)
            ));
        }

        {
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r"\w")?;

            let mut nfa = NFA::new();
            nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

            let char_vec: Vec<u8> = (b'0'..=b'9')
                .chain(b'A'..=b'Z')
                .chain(b'a'..=b'z')
                .chain(std::iter::once(b'_'))
                .collect();
            assert!(has_transition(
                &nfa,
                NFA::START_STATE,
                NFA::ACCEPT_STATE,
                Transition::convert_char_vec_to_symbol_onehot_encoding(char_vec)
            ));
        }

        {
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r"\D")?;

            let mut nfa = NFA::new();
            let nfa_result = nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE);
            assert!(nfa_result.is_err());
        }

        Ok(())
    }

    #[test]
    fn test_concat_simple() -> Result<()> {
        let mut parser = RegexParser::new();
        let parsed_ast = parser.parse_into_ast(r"<\d>")?;

        let mut nfa = NFA::new();
        nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

        assert!(has_transition(
            &nfa,
            NFA::START_STATE,
            State(2),
            Transition::convert_char_to_symbol_onehot_encoding('<')
        ));
        assert!(has_transition(&nfa, State(2), State(3), DIGIT_TRANSITION));
        assert!(has_transition(
            &nfa,
            State(3),
            NFA::ACCEPT_STATE,
            Transition::convert_char_to_symbol_onehot_encoding('>')
        ));

        Ok(())
    }

    #[test]
    fn test_alternation_simple() -> Result<()> {
        let mut parser = RegexParser::new();
        let parsed_ast = parser.parse_into_ast(r"\d|a|bcd")?;

        let mut nfa = NFA::new();
        nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

        assert!(has_transition(
            &nfa,
            NFA::START_STATE,
            State(2),
            EPSILON_TRANSITION
        ));
        assert!(has_transition(&nfa, State(2), State(3), DIGIT_TRANSITION));
        assert!(has_transition(
            &nfa,
            State(3),
            NFA::ACCEPT_STATE,
            EPSILON_TRANSITION
        ));

        assert!(has_transition(
            &nfa,
            NFA::START_STATE,
            State(4),
            EPSILON_TRANSITION
        ));
        assert!(has_transition(
            &nfa,
            State(4),
            State(5),
            Transition::convert_char_to_symbol_onehot_encoding('a')
        ));
        assert!(has_transition(
            &nfa,
            State(5),
            NFA::ACCEPT_STATE,
            EPSILON_TRANSITION
        ));

        assert!(has_transition(
            &nfa,
            NFA::START_STATE,
            State(6),
            EPSILON_TRANSITION
        ));
        assert!(has_transition(
            &nfa,
            State(6),
            State(8),
            Transition::convert_char_to_symbol_onehot_encoding('b')
        ));
        assert!(has_transition(
            &nfa,
            State(8),
            State(9),
            Transition::convert_char_to_symbol_onehot_encoding('c')
        ));
        assert!(has_transition(
            &nfa,
            State(9),
            State(7),
            Transition::convert_char_to_symbol_onehot_encoding('d')
        ));
        assert!(has_transition(
            &nfa,
            State(7),
            NFA::ACCEPT_STATE,
            EPSILON_TRANSITION
        ));

        Ok(())
    }

    #[test]
    fn test_repetition() -> Result<()> {
        let a_transition = Transition::convert_char_to_symbol_onehot_encoding('a');
        let range_bound_state = State(2);

        {
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r"a{0,3}")?;

            let mut nfa = NFA::new();
            nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

            assert!(has_transition(
                &nfa,
                NFA::START_STATE,
                range_bound_state.clone(),
                EPSILON_TRANSITION
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                State(3),
                a_transition
            ));
            assert!(has_transition(
                &nfa,
                State(3),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));
            assert!(has_transition(&nfa, State(3), State(4), a_transition));
            assert!(has_transition(
                &nfa,
                State(4),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));
            assert!(has_transition(&nfa, State(4), State(5), a_transition));
            assert!(has_transition(
                &nfa,
                State(5),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));

            assert_eq!(nfa.states.len(), 6);
        }

        {
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r"a{0,1}")?;

            let mut nfa = NFA::new();
            nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

            assert!(has_transition(
                &nfa,
                NFA::START_STATE,
                range_bound_state.clone(),
                EPSILON_TRANSITION
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                State(3),
                a_transition
            ));
            assert!(has_transition(
                &nfa,
                State(3),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));

            assert_eq!(nfa.states.len(), 4);
        }

        {
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r"a*")?;

            let mut nfa = NFA::new();
            nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

            assert!(has_transition(
                &nfa,
                NFA::START_STATE,
                range_bound_state.clone(),
                EPSILON_TRANSITION
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                range_bound_state.clone(),
                a_transition
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));

            assert_eq!(nfa.states.len(), 3);
        }

        {
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r"a+")?;

            let mut nfa = NFA::new();
            nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

            assert!(has_no_transition(
                &nfa,
                NFA::START_STATE,
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));
            assert!(has_transition(
                &nfa,
                NFA::START_STATE,
                range_bound_state.clone(),
                a_transition
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                range_bound_state.clone(),
                a_transition
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));

            assert_eq!(nfa.states.len(), 3);
        }

        {
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r"a{1,}")?;

            let mut nfa = NFA::new();
            nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

            assert!(has_no_transition(
                &nfa,
                NFA::START_STATE,
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));
            assert!(has_transition(
                &nfa,
                NFA::START_STATE,
                range_bound_state.clone(),
                a_transition
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                range_bound_state.clone(),
                a_transition
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));

            assert_eq!(nfa.states.len(), 3);
        }

        {
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r"a{3,}")?;

            let mut nfa = NFA::new();
            nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

            assert!(has_no_transition(
                &nfa,
                NFA::START_STATE,
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));
            assert!(has_transition(
                &nfa,
                NFA::START_STATE,
                State(3),
                a_transition
            ));
            assert!(has_transition(&nfa, State(3), State(4), a_transition));
            assert!(has_transition(
                &nfa,
                State(4),
                range_bound_state.clone(),
                a_transition
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                range_bound_state.clone(),
                a_transition
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));

            assert_eq!(nfa.states.len(), 5);
        }

        {
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r"a{3}")?;

            let mut nfa = NFA::new();
            nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

            assert!(has_no_transition(
                &nfa,
                NFA::START_STATE,
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));
            assert!(has_transition(
                &nfa,
                NFA::START_STATE,
                State(3),
                a_transition
            ));
            assert!(has_transition(&nfa, State(3), State(4), a_transition));
            assert!(has_transition(
                &nfa,
                State(4),
                range_bound_state.clone(),
                a_transition
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));

            assert_eq!(nfa.states.len(), 5);
        }

        {
            let mut parser = RegexParser::new();
            let parsed_ast = parser.parse_into_ast(r"a{3,6}")?;

            let mut nfa = NFA::new();
            nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

            assert!(has_no_transition(
                &nfa,
                NFA::START_STATE,
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));
            assert!(has_transition(
                &nfa,
                NFA::START_STATE,
                State(3),
                a_transition
            ));
            assert!(has_transition(&nfa, State(3), State(4), a_transition));
            assert!(has_transition(
                &nfa,
                State(4),
                range_bound_state.clone(),
                a_transition
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                State(5),
                a_transition
            ));
            assert!(has_transition(
                &nfa,
                State(5),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));
            assert!(has_transition(&nfa, State(5), State(6), a_transition));
            assert!(has_transition(&nfa, State(6), State(7), a_transition));
            assert!(has_transition(
                &nfa,
                State(7),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));
            assert!(has_transition(
                &nfa,
                range_bound_state.clone(),
                NFA::ACCEPT_STATE,
                EPSILON_TRANSITION
            ));

            assert_eq!(nfa.states.len(), 8);
        }

        Ok(())
    }

    #[test]
    fn test_group() -> Result<()> {
        let mut parser = RegexParser::new();
        let parsed_ast = parser.parse_into_ast(r"(\s|\d)+")?;

        let mut nfa = NFA::new();
        nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;
        println!("{:?}", nfa);

        assert!(has_transition(
            &nfa,
            NFA::START_STATE,
            State(3),
            EPSILON_TRANSITION
        ));
        assert!(has_transition(&nfa, State(3), State(4), SPACE_TRANSITION));
        assert!(has_transition(&nfa, State(4), State(2), EPSILON_TRANSITION));
        assert!(has_transition(
            &nfa,
            NFA::START_STATE,
            State(5),
            EPSILON_TRANSITION
        ));
        assert!(has_transition(&nfa, State(5), State(6), DIGIT_TRANSITION));
        assert!(has_transition(&nfa, State(6), State(2), EPSILON_TRANSITION));

        assert!(has_transition(&nfa, State(2), State(7), EPSILON_TRANSITION));
        assert!(has_transition(&nfa, State(7), State(8), SPACE_TRANSITION));
        assert!(has_transition(&nfa, State(8), State(2), EPSILON_TRANSITION));
        assert!(has_transition(&nfa, State(2), State(9), EPSILON_TRANSITION));
        assert!(has_transition(&nfa, State(9), State(10), DIGIT_TRANSITION));
        assert!(has_transition(
            &nfa,
            State(10),
            State(2),
            EPSILON_TRANSITION
        ));

        assert!(has_transition(
            &nfa,
            State(2),
            NFA::ACCEPT_STATE,
            EPSILON_TRANSITION
        ));

        assert_eq!(nfa.states.len(), 11);

        Ok(())
    }

    #[test]
    fn test_bracketed() -> Result<()> {
        let mut parser = RegexParser::new();
        let parsed_ast = parser.parse_into_ast(r"[a-c3-9[A-X]]")?;

        let mut nfa = NFA::new();
        nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

        assert!(has_transition(
            &nfa,
            NFA::START_STATE,
            State(2),
            Transition::convert_char_range_to_symbol_onehot_encoding(Some((b'a', b'c')))
        ));
        assert!(has_transition(
            &nfa,
            State(2),
            State(3),
            Transition::convert_char_range_to_symbol_onehot_encoding(Some((b'3', b'9')))
        ));
        assert!(has_transition(
            &nfa,
            State(3),
            NFA::ACCEPT_STATE,
            Transition::convert_char_range_to_symbol_onehot_encoding(Some((b'A', b'X')))
        ));

        Ok(())
    }

    #[test]
    fn test_floating_point_regex() -> Result<()> {
        let mut parser = RegexParser::new();
        let parsed_ast = parser.parse_into_ast(r"\-{0,1}[0-9]+\.\d+")?;

        let mut nfa = NFA::new();
        nfa.add_ast_to_nfa(&parsed_ast, NFA::START_STATE, NFA::ACCEPT_STATE)?;

        println!("{:?}", nfa);

        assert!(has_transition(
            &nfa,
            NFA::START_STATE,
            State(3),
            EPSILON_TRANSITION
        ));
        assert!(has_transition(
            &nfa,
            State(3),
            State(4),
            Transition::convert_char_to_symbol_onehot_encoding('-')
        ));
        assert!(has_transition(&nfa, State(4), State(2), EPSILON_TRANSITION));

        assert!(has_transition(&nfa, State(2), State(6), DIGIT_TRANSITION));
        assert!(has_transition(&nfa, State(6), State(6), DIGIT_TRANSITION));

        assert!(has_transition(&nfa, State(6), State(5), EPSILON_TRANSITION));

        assert!(has_transition(
            &nfa,
            State(5),
            State(7),
            Transition::convert_char_to_symbol_onehot_encoding('.')
        ));
        assert!(has_transition(&nfa, State(7), State(8), DIGIT_TRANSITION));
        assert!(has_transition(&nfa, State(8), State(8), DIGIT_TRANSITION));
        assert!(has_transition(&nfa, State(8), State(1), EPSILON_TRANSITION));

        assert_eq!(nfa.states.len(), 9);

        Ok(())
    }

    fn has_transition(nfa: &NFA, from: State, to: State, onehot_trans: u128) -> bool {
        if from.0 >= nfa.states.len() || to.0 >= nfa.states.len() {
            return false;
        }
        if false == nfa.transitions.contains_key(&from) {
            return false;
        }
        for trans in nfa.transitions.get(&from).unwrap() {
            if to != trans.to {
                continue;
            }
            if trans.symbol_onehot_encoding == onehot_trans {
                return true;
            }
        }
        false
    }

    fn has_no_transition(nfa: &NFA, from: State, to: State, onehot_trans: u128) -> bool {
        false == has_transition(nfa, from, to, onehot_trans)
    }

    #[test]
    fn nfa_epsilon_closure() {
        let mut nfa = NFA::new();
        for _ in 0..=10 {
            _ = nfa.new_state();
        }
        nfa.add_epsilon_transition(NFA::START_STATE, NFA::ACCEPT_STATE);
        nfa.add_epsilon_transition(NFA::ACCEPT_STATE, State(2));
        nfa.add_epsilon_transition(NFA::START_STATE, State(2));
        nfa.add_transition(
            State(2),
            State(3),
            Transition::convert_char_to_symbol_onehot_encoding('a'),
        );
        nfa.add_epsilon_transition(State(3), State(5));
        nfa.add_epsilon_transition(State(3), State(4));
        nfa.add_epsilon_transition(State(4), State(6));
        nfa.add_epsilon_transition(State(6), State(3));

        let closure = nfa.epsilon_closure(&vec![NFA::START_STATE]);
        assert_eq!(closure.len(), 3);
        assert_eq!(closure.contains(&NFA::START_STATE), true);
        assert_eq!(closure.contains(&NFA::ACCEPT_STATE), true);
        assert_eq!(closure.contains(&State(2)), true);

        let closure = nfa.epsilon_closure(&vec![State(3)]);
        assert_eq!(closure.len(), 4);
        assert_eq!(closure.contains(&State(3)), true);
        assert_eq!(closure.contains(&State(4)), true);
        assert_eq!(closure.contains(&State(5)), true);
        assert_eq!(closure.contains(&State(6)), true);
    }
}

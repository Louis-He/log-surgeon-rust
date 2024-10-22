use crate::parser::ast_node::ast_node::ASTNode;

#[derive(Debug)]
pub(crate) struct ASTNodeUnion {
    m_op1: Box<ASTNode>,
    m_op2: Box<ASTNode>,
}

impl PartialEq for ASTNodeUnion {
    fn eq(&self, other: &Self) -> bool {
        self.m_op1 == other.m_op1 && self.m_op2 == other.m_op2
    }
}

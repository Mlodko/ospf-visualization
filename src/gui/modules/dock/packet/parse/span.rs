#[derive(Clone, Copy)]
pub struct Span {
    pub start: usize,
    pub end: usize
}

impl core::fmt::Debug for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}, {})", self.start, self.end)
    }
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
    
    pub fn compose(&self, other: &Span) -> Span {
        Span {
            start: self.start + other.start,
            end: self.end + other.start
        }
    }
    
    pub fn offset(&self, offset: usize) -> Span {
        Span {
            start: self.start + offset,
            end: self.end + offset
        }
    }
}

#[derive(Debug, Clone)]
pub struct Field<T> {
    pub span: Span,
    pub value: T
}

impl<T> Field<T> {
    pub fn new(span: Span, value: T) -> Self {
        Field { span, value }
    }
}

impl<T> PartialEq for Field<T>
where T: PartialEq {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
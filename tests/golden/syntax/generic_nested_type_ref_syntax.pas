program GenericNestedTypeRefSyntax;

// After the type arguments in a type position, `.` and names continue the type as a
// NESTED_TYPE_REF whose first child is the instantiation (grammar.md 1.3): the last name may
// have type arguments of its own, and the form repeats (`A<X>.B<Y>.C`). In an expression's type
// arguments the same nodes are read. Recovery: no name after `.` is E0405, a missing `>` after
// the last name is E0400. The names are not declared, so the analysis reports them.

var
  A: TList<Integer>.TEnumerator;
  B: TOuter<Integer>.TInner<string>.TPair;
  C: TMap<string, TList<Integer>.TEnumerator>;
  D: TOuter<Integer>.TInner.TDeep<Byte>;
  P: TObject;
  E: TOuter<Integer>.;
  F: TOuter<Integer>.TInner<Byte;
begin
  P := TList<TOuter<Integer>.TInner<string>.TPair>.Create;
end.

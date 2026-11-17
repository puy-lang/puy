// Generic types of the same name in two units in `uses` (generics.md 2.3): different numbers of
// type parameters are different names (`TPair<A, B>` and `TPair<T>`),
// the same number is E2502 with a fix to qualify each, and the qualified names are found.
program Main;

uses Pairs, Boxes;

var
  P2: TPair<Integer, string>;
  P1: TPair<Integer>;
  B: TBox<Integer>;
  BP: Pairs.TBox<Integer>;
  BB: Boxes.TBox<Integer>;
begin
  P2.Second := 'two';
  P1.Only := 1;
  BP.Value := 2;
  BB.Item := 3;
  WriteLn(P2.Second, P1.Only, BP.Value, BB.Item);
end.

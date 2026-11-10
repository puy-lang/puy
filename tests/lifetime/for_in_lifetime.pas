program ForInLifetime;

// Where the lifetime instructions go for `for..in` (types.md chapter 5): the collection is
// read into a hidden register with `AssignRef` before the loop, so it lives until the routine
// ends, and a managed element is read into the loop variable with a counted read.

type
  TNames = array of string;

procedure Show(const Names: TNames);
begin
  for var S in Names do
    WriteLn(S);
end;

var
  Names: TNames;

begin
  Names := ['a', 'b'];
  for var S in Names do
  begin
    Names := nil;
    WriteLn(S);
  end;
  Show(['c']);
end.

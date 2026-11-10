program ForInVars;

// Loop variables of `for..in` (types.md chapter 5, semantics.md 4.5): `for var X` takes the
// element type or a written type, is a new variable on each iteration, and may be a managed
// type. A global variable in the main block holds the element of the last entry after the
// loop, even if a routine called from the body changes it, and keeps its value if the body
// never runs.

type
  TNames = TArray<string>;

var
  G: string;
  Names: TNames;

procedure Clobber;
begin
  G := 'clobbered';
end;

procedure Each(const A: array of string);
var
  S: string;
begin
  for S in A do
    Write('[', S, ']');
  WriteLn;
end;

function Joined(Words: TNames): string;
begin
  Result := '';
  for var W in Words do
    Result := Result + W;
end;

begin
  Names := ['ab', 'cd', 'ef'];
  for var N in Names do
    Write(N, ' ');
  WriteLn;
  for var C: string in 'xyz' do
    Write(C + C, ' ');
  WriteLn;
  var Small: TArray<Integer> := [1, 2, 3];
  for var I: Int64 in Small do
    Write(I * 10000000000, ' ');
  WriteLn;
  Each(Names);
  Each(['p', 'q']);
  WriteLn(Joined(Names));

  G := 'start';
  for G in Names do
    Clobber;
  WriteLn('after: ', G);
  for G in Names do
  begin
    if G = 'cd' then
      Break;
    Clobber;
  end;
  WriteLn('after break: ', G);
  for G in Names do
    if G = 'zz' then
      Break;
  WriteLn('after no break: ', G);
  Names := nil;
  G := 'kept';
  for G in Names do
    Clobber;
  WriteLn('empty: ', G);
end.

program RecordGlobals;

// Global records and static arrays with initial values written as constant shapes (in
// `<init>`), arrays of records as value and const parameters, and assigning an element from
// a typed constant (types.md chapter 4).

type
  TP = record X: Integer; S: string; end;
  TA = array[1..2] of TP;
const
  K: TA = ((X: 1; S: 'a'), (X: 2; S: 'b'));
var
  Gl: TP = (X: 5; S: 'g');
  Arr: TA = ((X: 7; S: 'x'), (X: 8; S: 'y'));
function Sum(const A: TA): Integer;
begin
  Result := A[1].X + A[2].X;
end;
procedure Mod1(A: TA);
begin
  A[1].S := 'changed';
  WriteLn(A[1].S, Arr[1].S);
end;
begin
  WriteLn(Gl.S, Gl.X, Arr[2].S, Sum(K), Sum(Arr));
  Mod1(Arr);
  Arr[2] := K[1];
  Arr[2].S := Arr[2].S + '!';
  WriteLn(Arr[2].S, K[1].S, Length(K), Low(K), High(K));
end.

program CopyAnyArray;

// `Copy` takes an open array and a static array too and makes a dynamic array (types.md
// chapter 7; Delphi takes only dynamic arrays). A default static array has its elements.

type
  TThree = array[1..3] of Integer;

procedure Show(const Title: string; const A: array of Integer);
var
  I: Integer;
begin
  Write(Title, ':');
  for I := 0 to High(A) do
    Write(' ', A[I]);
  WriteLn(' (', Length(A), ')');
end;

function Tail(const A: array of Integer): TArray<Integer>;
begin
  Result := Copy(A, 1);
end;

var
  T, Z: TThree;
begin
  T[1] := 4; T[2] := 5; T[3] := 6;
  Show('static', Copy(T));
  Show('static', Copy(T, 2, 1));
  Show('tail', Tail(T));
  Show('tail', Tail([7, 8, 9]));
  Z := Default(TThree);
  Show('default', Copy(Z));
  Show('default', Z);
  Show('negative', Copy(T, -5, 6));
end.

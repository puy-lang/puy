program Records;

uses
  System.SysUtils;

// Records, dynamic arrays and value semantics.

type
  TPerson = record
    Name: string;
    Age: Integer;
  end;

  TPeople = TArray<TPerson>;

procedure SortByAge(var People: TPeople);
var
  I, J: Integer;
  Tmp: TPerson;
begin
  // Insertion sort
  for I := 1 to High(People) do
  begin
    Tmp := People[I];
    J := I - 1;
    while (J >= 0) and (People[J].Age > Tmp.Age) do
    begin
      People[J + 1] := People[J];
      Dec(J);
    end;
    People[J + 1] := Tmp;
  end;
end;

function Person(const Name: string; Age: Integer): TPerson;
begin
  // Puy requires Result to be assigned before its fields are written.
  Result := Default(TPerson);
  Result.Name := Name;
  Result.Age := Age;
end;

function Oldest(const People: array of TPerson): TPerson;
begin
  Result := People[0];
  for var P in People do
    if P.Age > Result.Age then
      Result := P;
end;

var
  People: TPeople;
  Other: TPerson;

begin
  People := [Person('Alice', 31), Person('Bob', 25), Person('Carol', 42)];

  SortByAge(People);
  for var P in People do
    WriteLn(Format('%-6s %3d', [P.Name, P.Age]));

  WriteLn('Oldest: ', Oldest(People).Name);

  // Records are values: changing the copy leaves the array alone.
  Other := People[0];
  Other.Name := 'Changed';
  WriteLn(People[0].Name, ' / ', Other.Name);
end.

// A helper's method called on a member of a value (objects.md 10.4) works as it does on a variable
// holding that member: a field of a record (also through a `for … in` variable, a nested record, an
// element of a dynamic or static array, a function's result), and a field and a property of a
// class. A field of a subrange type finds its own type's helper first (10.4, step 3). These were read as a field of the record that has no such field (E0700) before.
program HelperOnMember;

type
  TSmall = 1..10;

  TSmallHelper = record helper for TSmall
    function Plus(N: Integer): Integer;
  end;

  TIntHelper = record helper for Integer
    function Twice: Integer;
  end;

  TItem = record
    Name: string;
    Count: Integer;
    Level: TSmall;
  end;

  TOuter = record
    Inner: TItem;
  end;

  TBox = class
    Name: string;
  private
    FItem: TItem;
  public
    property Item: TItem read FItem write FItem;
    property Title: string read Name;
  end;

function TIntHelper.Twice: Integer;
begin
  Result := Self * 2;
end;

function TSmallHelper.Plus(N: Integer): Integer;
begin
  Result := Self + N;
end;

function Make(const Name: string): TItem;
begin
  Result := Default(TItem);
  Result.Name := Name;
  Result.Count := 4;
end;

var
  X: TItem;
  O: TOuter;
  A: array of TItem;
  S: array[0..1] of TItem;
  B: TBox;

begin
  X.Name := 'apple';
  X.Count := 3;
  WriteLn(X.Name.StartsWith('ap'), ' ', X.Name.StartsWith('b'), ' ', X.Name.ToUpper, ' ',
    X.Count.Twice);
  X.Level := 7;
  WriteLn(X.Level.Plus(2), ' ', X.Level.Twice);
  var V := X.Name;
  WriteLn(V.StartsWith('ap'));
  O.Inner := X;
  WriteLn(O.Inner.Name.StartsWith('ap'), ' ', O.Inner.Name.IndexOf('p'), ' ',
    O.Inner.Count.Twice);
  A := [X, Make('banana')];
  for var Item in A do
    WriteLn(Item.Name, ' ', Item.Name.StartsWith('ba'), ' ', Item.Count.Twice);
  WriteLn(A[1].Name.EndsWith('na'), ' ', A[0].Count.Twice);
  S[0] := Make('cherry');
  WriteLn(S[0].Name.Contains('err'), ' ', S[0].Count.Twice);
  WriteLn(Make('date').Name.StartsWith('d'));
  B := TBox.Create;
  try
    B.Name := 'box';
    B.Item := Make('fig');
    WriteLn(B.Name.StartsWith('b'), ' ', B.Title.ToUpper, ' ', B.Item.Name.StartsWith('f'), ' ',
      B.Item.Count.Twice);
  finally
    B.Free;
  end;
end.

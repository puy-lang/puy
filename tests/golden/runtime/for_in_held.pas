program ForInHeld;

// `for..in` holds the collection's reference for the whole loop (types.md chapter 5): the
// loop still visits every element after the variable is set to `nil`, and a write to a static
// array or a record's array in the body is not seen (they are values). FPC reads the variable
// itself, so this is not a differential test.

type
  TInts = array of Integer;
  TTriple = array[0..2] of Integer;
  TBox = record
    Items: TTriple;
  end;

var
  D: TInts;
  T: TTriple;
  B: TBox;
  E: Integer;

begin
  D := [1, 2, 3];
  Write('nil:');
  for E in D do
  begin
    Write(' ', E);
    D := nil;
  end;
  WriteLn(' (', Length(D), ')');

  T := [1, 2, 3];
  Write('static:');
  for E in T do
  begin
    Write(' ', E);
    T[2] := 30;
  end;
  WriteLn(' (', T[2], ')');

  B.Items := [4, 5, 6];
  Write('field:');
  for E in B.Items do
  begin
    Write(' ', E);
    B.Items[2] := 60;
  end;
  WriteLn(' (', B.Items[2], ')');
end.

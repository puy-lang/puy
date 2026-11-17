program AttrPlaces;

// E3401 (attributes.md 1.2): attributes where none can be written, read on as usual (a constant,
// a `resourcestring`, an enumerated value, a nested routine, a `for var` variable, a member of a
// class body that is not a field, a method or a property, an item of the declaration part that
// is not a routine), on the body of a method or of a forward routine (pointing at the first
// declaration), and on a forward declaration of a class (pointing at the full declaration).

type
  MarkAttribute = class(TCustomAttribute)
  end;

  [Mark] TLater = class;

  TShape = class
  public
    [Mark] const Sides = 3;
    procedure Draw;
  end;

  TColor = (Red, [Mark] Green);

  TLater = class
  end;

const
  [Mark] Limit = 10;

resourcestring
  [Mark] Greeting = 'hello';

[Mark]
procedure TShape.Draw;
begin
end;

procedure Later(N: Integer); forward;

[Mark]
procedure Later(N: Integer);
var
  I: Integer;

  [Mark]
  procedure Inner;
  begin
  end;

begin
  for var [Mark] J := 1 to N do
    I := J;
  Inner;
  WriteLn(I);
end;

[Mark] var Stray: Integer;

begin
  Later(Limit);
  WriteLn(Greeting, Ord(TColor.Green), TShape.Sides);
end.

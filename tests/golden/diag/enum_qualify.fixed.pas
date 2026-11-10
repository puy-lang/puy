program EnumQualify;

// E0736 (types.md 8.1): an enumeration value written without its type. With one type
// holding the name the fix is safe; with several, each type is a suggestion. A name the
// program declares itself is that declaration, not an enumeration value. An anonymous
// enumeration has no name to qualify its values with.

type
  TColor = (Red, Green, Blue);
  TLight = (Off, Red, Amber);
  TFruit = (Apple, Pear);

var
  C: TColor;
  L: TLight;
  V: (P, Q);
  Pear: Integer;

procedure Local;
type
  TSide = (Left, Right);
var
  S: TSide;
begin
  S := TSide.Right;
  WriteLn(Ord(S));
end;

begin
  C := TColor.Green;
  L := TLight.Amber;
  C := Red;
  Pear := 1;
  WriteLn(Pear, Ord(TFruit.Pear));
  C := tcolor.BLUE;
  Local;
end.

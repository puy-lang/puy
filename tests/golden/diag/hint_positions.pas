program HintPositions;

// Hint directives in each position (attributes.md 6.1, 6.2): `deprecated` / `experimental` with
// and without a description after a routine, a type, a class (`end deprecated` and after its
// parent), a constant, a variable, a field, a `class var` and a property, each reported where it
// is used (W0011 / W0012, the member names qualified with their type, a line break of the
// description shown as a space); `platform` / `library` in the new positions are W0010 with a fix
// removing them. A declaration marked both ways gives both warnings, in this order. The last line
// is an ordinary error, read after them all.

type
  TVec = class(TRefObject)
    X: Integer deprecated;
    Y: Integer platform;
    class var Count: Integer experimental 'may go';
    property PX: Integer read X experimental;
  end platform;

  TOld = Integer deprecated 'use Integer';
  TBoth = Integer deprecated experimental;
  EOld = class(Exception) platform deprecated '''
    use
    EError
    ''';
  TLib = Integer library;

const
  Limit = 10 deprecated;

var
  Count: Integer experimental;

procedure Each(N: Integer); experimental;
begin
end;

var
  V: TVec;
  A: TOld;
  B: TBoth;
  E: EOld;
  I: Integer;
begin
  V := TVec.Create;
  A := V.X + V.PX + TVec.Count + Limit + Count;
  Each(A);
  B := 0;
  E := nil;
  WriteLn(A, B, E = nil, V.Y);
  I := 'not an integer';
end.

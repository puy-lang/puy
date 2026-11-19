// Comparisons read as type arguments (docs/spec/generics.md 1.4): each form gets the note
// that fits it, and the misread does not add a wrong number of arguments.
program GenericComparisonMisreads;

type
  TFoo = class
    constructor Create(X, Y: Boolean);
    procedure M(X, Y: Boolean);
  end;

constructor TFoo.Create(X, Y: Boolean);
begin
end;

procedure TFoo.M(X, Y: Boolean);
begin
end;

procedure One(X: Boolean);
begin
end;

function Two(X, Y: Boolean): Boolean;
begin
  Result := X and Y;
end;

procedure Over(X, Y: Boolean); overload;
begin
end;

procedure Over(X: Integer); overload;
begin
end;

procedure Pair(X, Y: Boolean);
begin
end;

function Cnt<T>: Integer;
begin
  Result := 1;
end;

var
  a, b, c, d: Integer;
  o: TFoo;
  P: procedure(X, Y: Boolean);

begin
  // Two comparisons separated by the comma of a call: no E0705 for `Pair`, `M`, `P`,
  // `Create` or `Over`
  Pair(a < b, c > (d));
  WriteLn(a < b, b > (c));
  o.M(a < b, c > (d));
  P := Pair;
  P(a < b, c > (d));
  o := TFoo.Create(a < b, c > (d));
  Over(a < b, c > (d));
  // Only the call holding the misread argument: `One` still gets its E0705
  One(Two(a < b, c > (d)), True);
  // Two chained comparisons
  WriteLn(a < b > (c));
  // `>=` written without a space after the type arguments
  if Cnt<Byte>=2 then
    WriteLn('two');
end.

program GenericBodyOps;

uses
  System.SysUtils;

// A generic body is checked once with its type parameters (generics.md 3.3, 3.4): a value of a
// type parameter without constraints can be declared, assigned, passed and returned, and
// `Default(T)` is its default value; any other use is E0762 with the note of its kind (writing,
// comparing, `nil`, members, `Free` and `FreeAndNil`, operators, conversions, `Ord`, `SizeOf`, a
// `for` control variable). With the `class` constraint, `nil` is allowed but `Free` still is not
// (a reference counted class can be passed). A variable of a type parameter read before it is
// assigned is E2000, whose fix writes `Default(T)`.

type
  TBag<T> = class
    FItem: T;
    procedure Show;
  end;
  TCheck<T> = class
    FItem: T;
    procedure Run;
    function Peek: T;
  end;
  TRefs<T: class> = class
    FItem: T;
    procedure Run;
  end;

procedure TBag<T>.Show;
begin
  WriteLn(FItem); // error: E0762
end;

procedure TCheck<T>.Run;
var
  Y: T;
  I: Integer;
begin
  Y := Default(T);
  if FItem = Y then
    Exit;
  FItem := nil;
  if Assigned(FItem) then
    Exit;
  FItem.Run;
  FItem.Free;
  Y := FItem + Y;
  Y := -FItem;
  I := FItem;
  Y := T(I);
  I := Ord(FItem);
  I := SizeOf(T);
  for Y := Y to Y do
    Exit;
  FreeAndNil(FItem);
end;

function TCheck<T>.Peek: T;
var
  Last: T;
begin
  Exit(Last);
end;

procedure TRefs<T>.Run;
begin
  FItem.Free;
  FItem := nil;
  WriteLn(FItem);
end;

begin
end.

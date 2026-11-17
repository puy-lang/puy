program GenericSetElements;

// A value of a type parameter cannot be a set element in a generic body (generics.md 3.3): an
// element or a bound of a set constructor, the element of `Include` / `Exclude` and the left
// operand of `in` before a set constructor are E0762, as `X in S` is, not a type mismatch with
// the base type of the set.

type
  TBag<T> = class
    FItem: T;
    procedure Run;
  end;

procedure TBag<T>.Run;
var
  S: set of Byte;
begin
  S := [FItem];
  S := [1, FItem..FItem];
  Include(S, FItem);
  Exclude(S, FItem);
  if FItem in S then
    Exit;
  if FItem in [FItem, 1] then
    Exit;
  WriteLn(S = [FItem]);
end;

procedure Show<T>(const X: T);
var
  S: set of Char;
begin
  S := ['a', X];
  WriteLn(S = []);
end;

begin
end.

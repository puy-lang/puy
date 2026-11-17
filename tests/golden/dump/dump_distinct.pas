program Hello;

// Distinct types (types.md 1.3) in the dumps: the `hir` shows `.type TUserId = type Int64` and
// the types by their names; `TBox<TUserId>` is another instantiation than `TBox<Int64>`
// (generics.md 4.1), named with the full name `Hello.TUserId` in the function table. A value of
// a distinct type is lowered as one of its root type.

type
  TUserId = type Int64;
  TSmall = type Byte;

  TBox<T> = class
    Value: T;
    function Get: T;
  end;

function TBox<T>.Get: T;
begin
  Result := Value;
end;

var
  A: TBox<TUserId>;
  B: TBox<Int64>;
  S: TSmall;
  N: Integer;
begin
  A := TBox<TUserId>.Create;
  B := TBox<Int64>.Create;
  A.Value := 1;
  B.Value := A.Get + 1;
  N := 200;
  S := N;
  WriteLn(A.Get, ' ', B.Get, ' ', S);
  A.Free;
  B.Free;
end.

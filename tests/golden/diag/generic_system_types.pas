program GenericSystemTypes;

// The generic procedural types of `System` (generics.md 2.3, 6.2) are found by the number of
// type arguments next to the `TProc` without them: both are used here, and the type mismatches
// name each. `TFunc` without type arguments and with too many are E0761, as is a type nested in
// a class written with type arguments when it is not generic. A generic interface declared
// forward needs its body (E0723).

type
  THolder = class
  public
    type
      TItem = record
        V: Integer;
      end;
  end;

  ILater<T> = interface;

var
  P: TProc;
  P1: TProc<Integer>;
  P2: TProc<Integer, string>;
  F: TFunc<Integer>;
  Bad: TFunc;
  TooMany: TProc<Integer, Integer, Integer, Integer, Integer>;
  Item: THolder.TItem<Integer>;
begin
  P := procedure
    begin
    end;
  P1 := procedure(Arg1: Integer)
    begin
    end;
  P := P1;
  P2 := P1;
  F := P;
end.

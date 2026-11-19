program Main;

// The cycle through an instantiation is E0727 also when the generic type is declared in a unit
// (the steps in the unit are shown there) or in an earlier `type` section of the program
// (types.md 1.1), and for a record nested in a class.

uses Boxes;

type
  TFoo<T> = record
    V: T;
  end;

type
  TBar = record
    A: TFoo<TBar>;
  end;
  TKeyed = record
    P: TPair<Integer, TKeyed>;
  end;
  TRow = record
    C: TCells<TRow>;
  end;
  TOuter = class
  type
    TInner = record
      A: TFoo<TInner>;
    end;
  end;

begin
end.

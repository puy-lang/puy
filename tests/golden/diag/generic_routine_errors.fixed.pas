program GenericRoutineErrors;

// The errors of generic routines and methods (generics.md 2.1, 1.2, 1.4, 2.2, 2.3, 4.2, 5.1):
// E0757 where type parameters cannot be (a virtual method, an interface's method, a
// constructor, a destructor, an operator, a property, a nested routine), a body's heading whose type
// parameters differ from the declaration, left out, or repeat the constraints (with the fixes),
// E0701 for a type parameter written twice or named as the class's, E0761 for the number of type
// arguments, for a routine or type that is not generic (E0700 for an undeclared type and E0708 for a
// routine named in a body's heading) and for a call written without parentheses
// before `+` (with the fix), E0759 for a type argument against a routine's constraint, and E0763
// for a generic routine made a value without type arguments.

type
  TFoo = class
    procedure Virt<T>; virtual;
    constructor Create<T>;
    destructor Destroy<T>; override;
    class operator Add<T>(const A, B: TFoo): TFoo;
    procedure Renamed<T>;
    procedure LeftOut<T>;
    procedure Plain;
    property Prop<T>: Integer;
  end;

  IFoo = interface
    procedure Method<T>;
  end;

  TBox<T> = class
    procedure Clash<T>;
  end;

  TShow = procedure(const X: Integer);

procedure TFoo.Renamed<T>;
begin
end;

procedure TFoo.LeftOut<T>;
begin
end;

procedure TFoo.Plain<T>;
begin
end;

procedure TFoo<T>.Plain;
begin
end;

procedure TMissing<T>.Run;
begin
end;

procedure TBox<T>.Clash<T>;
begin
end;

procedure Outer;

  procedure Inner<T>;
  begin
  end;

begin
end;

procedure Show<T>(const X: T);
begin
end;

function Zero<T>: Integer;
begin
  Result := 0;
end;

procedure Twice<T, T>;
begin
end;

procedure Fwd<T: class>; forward;

procedure Fwd<T>;
begin
end;

procedure Other<T>; forward;

procedure Other<T>;
begin
end;

procedure NotGeneric;
begin
end;

procedure NotGeneric<T>.Run;
begin
end;

var
  P: TShow;
  N: Integer;
begin
  P := Show;
  Show<Integer, string>(1);
  NotGeneric<Integer>;
  N := Zero<Integer>() + 1;
  Fwd<Integer>;
  P := Show<Integer>;
end.

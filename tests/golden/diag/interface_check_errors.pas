program InterfaceCheckErrors;

// Diagnostics of `is`, `as`, Supports and conversions by type name with interfaces
// (objects.md 7.6, 16.1): the second argument of Supports must name an interface (E0750); the
// `out` variable must be of the interface or an ancestor of it (E0702); a sealed class that does
// not implement the interface can never be it, nor an interface value be it (E0702); the first argument must be an instance, an
// interface value or (without the variable) a class reference (E0702); Supports takes 2 or 3
// arguments (E0705); the right side of `is` / `as` must be a class or an interface (E0702).

uses System.SysUtils;

type
  IFoo = interface
    procedure Foo;
  end;

  IBar = interface(IFoo)
    procedure Bar;
  end;

  TSealed = class sealed(TRefObject)
  end;

  TOpen = class(TRefObject)
  end;

  TOpenClass = class of TOpen;

  TPoint = record
    X: Integer;
  end;

var
  S: TSealed;
  P: TOpen;
  F: IFoo;
  B: IBar;
  O: TObject;
  N: Integer;
  K: TOpenClass;
begin
  // The interface argument
  WriteLn(Supports(P, TOpen));
  WriteLn(Supports(P, N));
  WriteLn(Supports(P, TPoint));
  // The `out` variable
  WriteLn(Supports(F, IFoo, O));
  WriteLn(Supports(F, IFoo, B));
  WriteLn(Supports(F, IBar, F));
  // A sealed class
  WriteLn(S is IFoo);
  F := S as IFoo;
  F := IFoo(S);
  WriteLn(Supports(S, IFoo, F));
  WriteLn(F is TSealed);
  S := F as TSealed;
  S := TSealed(F);
  // The instance argument and the number of arguments
  WriteLn(Supports(N, IFoo));
  WriteLn(Supports(K, IFoo, F));
  WriteLn(Supports(K, IFoo));
  WriteLn(Supports(F));
  WriteLn(Supports(F, IFoo, F, F));
  // `is` / `as`
  WriteLn(N is IFoo);
  WriteLn(P is TPoint);
  WriteLn(P is F);
end.

program ClassVarArgs;

// Diagnostics of fields of an instance passed to `var` parameters and of `FreeAndNil`
// (objects.md 2.2, 3.2, 4.3): E0741 for the same field of the same variable (or of `Self`,
// written with or without `Self.`) passed twice; different fields, or the same field of two
// variables, are not reported. `FreeAndNil` takes a variable of any class type, a descendant
// or `TObject` included, so those are not reported; another type is E0702, a value that is not
// a variable is E0706, and a wrong number of arguments is E0705.

uses
  System.SysUtils;

type
  TBase = class
  public
    F, G: Integer;
    procedure Run;
  end;

  TChild = class(TBase)
  end;

procedure Swap(var X, Y: Integer);
begin
end;

procedure TBase.Run;
begin
  Swap(F, Self.F);
  Swap(F, G);
end;

var
  O, P: TBase;
  C: TChild;
  Obj: TObject;
  N: Integer;
  S: string;
begin
  O := nil;
  P := nil;
  Swap(O.F, O.F);
  Swap(O.F, O.G);
  Swap(O.F, P.F);
  FreeAndNil(C);
  FreeAndNil(Obj);
  FreeAndNil(O);
  FreeAndNil(N);
  FreeAndNil(S);
  FreeAndNil(TBase.Create);
  FreeAndNil(O, P);
end.

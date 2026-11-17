program ClassValueErrors;

// Class values where they cannot be used (objects.md 2.7, 2.8, 3.2, 4.3): arithmetic and ordering
// operators on instances (E0703), conversions by type name between a class reference and an
// interface (E0702), `FreeAndNil` of an interface and of a class reference (E0702), and an
// instance of a descendant passed to a `var` parameter of the ancestor's type (E0702).

uses
  System.SysUtils;

type
  IFoo = interface
    procedure Run;
  end;

  TShape = class
  end;

  TShapeClass = class of TShape;

  TCircle = class(TShape)
  end;

procedure Reset(var S: TShape);
begin
  S := nil;
end;

var
  A, B: TShape;
  C: TCircle;
  K: TShapeClass;
  I: IFoo;
begin
  A := TShape.Create;
  B := TShape.Create;
  C := TCircle.Create;
  K := TShape;
  I := nil;
  WriteLn(A + B);
  WriteLn(A < B);
  I := IFoo(K);
  K := TShapeClass(I);
  FreeAndNil(I);
  FreeAndNil(K);
  Reset(C);
end.

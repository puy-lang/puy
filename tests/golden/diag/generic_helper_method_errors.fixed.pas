program GenericHelperMethodErrors;

// The errors of generic methods of helpers (generics.md 1.2, 2.1, 5.2; objects.md 10.2, 10.4):
// a virtual method, a constructor and an operator with type parameters are the members a helper
// cannot have (E0753, E0755) before E0757; E0744 for a record helper's class method without
// `static`; the body's heading with other names, without the type parameters and with the
// constraints (E0757 with a fix); E0763 for conflicting inferred types; E0759 for an inferred
// type argument against the constraint; E0754 for generic methods of one name in two helpers.

type
  TBox = class
    Value: Integer;
  end;

  TBoxHelper = class helper for TBox
    function Pick<T>(const A, B: T): T;
    function Keep<T>(const A: T): T;
    function Same<T: class>(const A, B: T): Boolean;
    procedure Spin<T>; virtual;
    constructor Make<T>;
    class operator Add<T>(const A, B: TBox): TBox;
    function Twin<T>(const A: T): T;
  end;

  TOtherBoxHelper = class helper for TBox
    function Twin<T>(const A: T): T;
  end;

  TPoint = record
    X: Integer;
  end;

  TPointHelper = record helper for TPoint
    class function Pair<T>(const A, B: T): TArray<T>;
  end;

function TBoxHelper.Pick<T>(const A, B: U): U;
begin
  Result := A;
end;

function TBoxHelper.Keep<T>(const A: T): T;
begin
  Result := A;
end;

function TBoxHelper.Same<T>(const A, B: T): Boolean;
begin
  Result := A = B;
end;

function TBoxHelper.Twin<T>(const A: T): T;
begin
  Result := A;
end;

function TOtherBoxHelper.Twin<T>(const A: T): T;
begin
  Result := A;
end;

class function TPointHelper.Pair<T>(const A, B: T): TArray<T>;
begin
  Result := [A, B];
end;

var
  B: TBox;
begin
  B := TBox.Create;
  WriteLn(B.Pick(1, 'a'));
  WriteLn(B.Same(1, 2));
  WriteLn(B.Twin(1));
  B.Free;
end.

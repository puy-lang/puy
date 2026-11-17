program DumpClassRefs;

// The dumps of class references and class members (objects.md 15.1-15.3): `.classvar`, class and
// static methods, `class of` types; `create-of` with a virtual constructor and `classref` in the
// hir; `NewOf`, `CallVirtualClass`, `ClassType` and the class reference loaded as `class …` in
// the ir; the abstract entry's name and the `class var`'s global in the bytecode.

type
  TShape = class abstract
    class var Count: Integer;
    constructor Create; virtual;
    function Area: Integer; virtual; abstract;
    class function Name: string; virtual;
    class function Twice(N: Integer): Integer; static;
  end;
  TShapeClass = class of TShape;

constructor TShape.Create;
begin
  Inc(Count);
end;

class function TShape.Name: string;
begin
  Result := ClassName;
end;

class function TShape.Twice(N: Integer): Integer;
begin
  Result := N * 2;
end;

var
  C: TShapeClass;
  S: TShape;
begin
  C := TShape;
  S := C.Create;
  WriteLn(C.Name, S.Name, TShape.Twice(TShape.Count));
  S.Free;
end.

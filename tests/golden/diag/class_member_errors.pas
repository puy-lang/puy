program ClassMemberErrors;

// Errors about the members of classes and class references (objects.md 2.2, 2.3, 2.8, 2.10,
// 4.3): the instance members in a class method and a static method (E0700, with the declaration),
// `Self` in a static method, an instance member through a class reference, a class's name used as
// a value where no class reference is expected (E0708), a class reference of a type that is not a
// class (E0733), assigning an ancestor's class reference to a descendant's (E0702), comparing
// class references of unrelated classes (E0703), a virtual static method (E0744), and `class
// constructor` / `class destructor` (E3200).

type
  TBase = class
    Value: Integer;
    class var Total: Integer;
    procedure Run;
    class procedure Make;
    class procedure Build; static;
    class procedure Twice; virtual; static;
    class constructor Init;
    class destructor Done;
  end;
  TChild = class(TBase)
  end;
  TOther = class
  end;
  TBaseClass = class of TBase;
  TChildClass = class of TChild;
  TOtherClass = class of TOther;
  TIntClass = class of Integer;

procedure TBase.Run;
begin
end;

class procedure TBase.Make;
begin
  Value := 1;
  Run;
  Self.Run;
  Inc(Total);
end;

class procedure TBase.Build;
begin
  WriteLn(Self.ClassName);
  Value := 2;
  Make;
  Inc(Total);
end;

class procedure TBase.Twice;
begin
end;

var
  B: TBaseClass;
  C: TChildClass;
  O: TOtherClass;
  I: Integer;
begin
  B := TChild;
  B.Run;
  I := B.Value;
  TBase.Run;
  I := TBase;
  C := B;
  WriteLn(B = O);
  WriteLn(TBase.ClassType.ClassName);
end.

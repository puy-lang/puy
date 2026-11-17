program MemberReservedNames;

// The reserved names (`Self`, `Result`, `Exit`, `Break`, `Continue`, `True`, `False`) cannot name
// a member of a class or a record either (objects.md 2.3, semantics.md 2.3): a field, a method, a
// property, a `class var` and a record field are each E0701.

type
  TFoo = class
    Self: Integer;
    FValue: Integer;
    procedure Result;
    property Exit: Integer read FValue;
  public
    class var True: Integer;
  end;

  TRec = record
    Break: Integer;
  end;

procedure TFoo.Result;
begin
end;

begin
end.

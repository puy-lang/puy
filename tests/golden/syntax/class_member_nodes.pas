program ClassMemberNodes;

// The CST of the class members not in class_syntax (objects.md chapter 1, grammar.md 2.2): the
// modifiers `sealed` / `abstract`, a `class var` section, a class method, an indexed property
// (`PROPERTY_PARAMS`) and a field with an attribute (`ATTRIBUTE_LIST`, `ATTRIBUTE`).

type
  TNode = class(TRefObject)
  end;

  TBase = class abstract
  end;

  TList = class sealed(TBase)
  class var
    Count: Integer;
  public
    [weak] Parent: TNode;
    class procedure Reset;
    function GetItem(I: Integer): Integer;
    property Items[I: Integer]: Integer read GetItem;
  end;

class procedure TList.Reset;
begin
  Count := 0;
end;

function TList.GetItem(I: Integer): Integer;
begin
  Result := I;
end;

begin
  TList.Reset;
end.

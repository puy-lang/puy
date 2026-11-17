program ClassParentOrder;

// A class's fields are numbered after its parent's, so the parent must be fully declared before
// the class (docs/spec/objects.md chapter 1): each form is E0723 rather than giving a field of the
// class the slot of a field of the parent. Inheriting from itself stays E0727.

type
  // Declared later in the section
  TLaterChild = class(TLater)
    Y: Integer;
  end;
  TLater = class
    X: Integer;
  end;

  // Only forward-declared so far
  TFwd = class;
  TFwdChild = class(TFwd)
    Y: Integer;
  end;
  TFwd = class
    X: Integer;
  end;

  // An instantiation of a generic class declared later in the section
  TBoxChild = class(TBox<Integer>)
    Y: Integer;
  end;
  TBox<T> = class
    X: T;
  end;

  // The enclosing class, whose body is not complete yet
  TOuter = class
    X: Integer;
    type
      TInner = class(TOuter)
        Y: Integer;
      end;
  end;

  // Methods only: still rejected, as the rule does not depend on the members
  TMethChild = class(TMeth)
  end;
  TMeth = class
    procedure Run; virtual; abstract;
  end;

  // An exception class declared later
  EChild = class(ELater);
  ELater = class(Exception);

  // Inheriting from itself
  TSelf = class(TSelf)
  end;

  // Declared before: fine
  TBase = class
    X: Integer;
  end;
  TDerived = class(TBase)
    Y: Integer;
  end;

var
  C: TLaterChild;
begin
  // The class falls back to `TObject`, so the parent's field is not found (E0700)
  C := TLaterChild.Create;
  C.X := 1;
  C.Free;
end.

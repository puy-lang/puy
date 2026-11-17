program GenericDeclKinds;

// The forms of generic declarations that are E0757 (generics.md 2.1, 2.4, 1.2): a kind of type
// that cannot be generic, a generic type inside a routine, a type parameter as the parent, an
// implementation heading whose type parameters differ from the declaration (with the fix that
// writes them as declared) or repeat its constraints (with the fix that removes them), and a full
// declaration whose type parameters differ from its forward declaration. A forward declaration
// without a full declaration is E0723.

type
  TBase = class
  end;
  TAlias<T> = TBase;
  TColor<T> = (Red, Green);
  TDigit<T> = 0..9;
  TBits<T> = set of Byte;
  TBaseClass<T> = class of TBase;
  TBaseHelper<T> = class helper for TBase
  end;
  TWrong<T> = class(T)
  end;
  TNode<T> = class;
  TNode<U> = class
  end;
  TLonely<T> = class;
  TBox<T> = class
    procedure Renamed;
    procedure Fewer;
    procedure Constrained;
  end;

procedure TBox<U>.Renamed;
begin
end;

procedure TBox<T, U>.Fewer;
begin
end;

procedure TBox<T: class>.Constrained;
begin
end;

procedure Run;
type
  TLocal<T> = record
    Value: T;
  end;
begin
end;

begin
  Run;
end.

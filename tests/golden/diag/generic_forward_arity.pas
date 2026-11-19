program GenericForwardArity;

// A forward declaration is completed only by a declaration with the same number of type
// parameters (generics.md 2.1, 2.3): `TPair<T, U>` is another type, so the forward declaration
// `TPair<T>` has no full declaration (E0723), not E0757. A forward declaration followed by both
// arities is completed by the one with its own number. The attributes of a forward declaration
// (E3401) point at the full declaration only when it has the same number (attributes.md 1.2).

type
  TAttr = class(TCustomAttribute)
  end;

  TPair<T> = class;
  TPair<T, U> = class
  end;

  IShape<T> = interface;
  IShape<T, U> = interface
  end;

  TCell<T> = class;
  TCell<T, U> = class
    Left: TCell<T>;
  end;
  TCell<T> = class
    Right: TCell<T, T>;
  end;

  [TAttr]
  TTagged<T> = class;
  TTagged<T, U> = class
  end;

  [TAttr]
  TMarked<T> = class;
  TMarked<T> = class
  end;

begin
end.

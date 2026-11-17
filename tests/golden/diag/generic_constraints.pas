program GenericConstraints;

// The constraint declarations that are E0758 (generics.md 1.1, 3.2, 2.1): a type that is neither
// a class type, an interface type nor `NotRefCounted`; a type parameter as a constraint;
// combinations that cannot be met; a second class type; `constructor` without `class` (fix:
// insert `class, `); a constraint listed twice (fix: remove it); and a full declaration whose
// constraints differ from its forward declaration. The combinations where one constraint includes
// the other are accepted.

type
  IHasName = interface
    function GetName: string;
  end;
  TBase = class
  end;
  TOther = class
  end;
  TPoint = record
    X, Y: Integer;
  end;
  TNotType<T: Integer> = class
  end;
  TNotRecord<T: TPoint> = class
  end;
  TOfParam<T; U: T> = record
  end;
  TClassRecord<T: class, record> = class
  end;
  TRecordCtor<T: record, constructor> = class
  end;
  TRecordIntf<T: record, IHasName> = class
  end;
  TTwoClasses<T: TBase, TOther> = class
  end;
  TCtorAlone<T: constructor> = class
  end;
  TListedTwice<T: class, NotRefCounted, class> = class
  end;
  TFwd<T: class> = class;
  TFwd<T: TBase> = class
  end;
  // Accepted: one constraint includes the other, and constraints shared by a group
  TIncluded<A: class, TBase; B: record, NotRefCounted; C: TBase, NotRefCounted> = class
  end;
  TShared<K, V: class, constructor; W: IHasName> = record
  end;
  TFwdOnly<T: class> = class;
  TFwdOnly<T> = class
  end;

begin
end.

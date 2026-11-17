program AttrCtorOverloads;

// The constructor of an attribute class is chosen as for `TFoo.Create(…)` (attributes.md 4.2):
// two overloads equally good for `(1, 1)` are E0734, a type name as an argument is E0708, and a
// class without its own constructor uses its parent's. An attribute class is reference counted,
// so `Free` on one is E3019 (4.1).

type
  PairAttribute = class(TCustomAttribute)
  public
    constructor Create(A: Integer; B: Double); overload;
    constructor Create(A: Double; B: Integer); overload;
  end;
  KidAttribute = class(PairAttribute)
  end;
  MarkAttribute = class(TCustomAttribute)
  end;

constructor PairAttribute.Create(A: Integer; B: Double);
begin
end;

constructor PairAttribute.Create(A: Double; B: Integer);
begin
end;

type
  [Pair(1, 1)] TA = class
  end;
  [Pair(Integer)] TB = class
  end;
  [Kid(1, 2.5)] TC = class
  end;
  [Kid(2.5, 1)] TD = class
  end;

begin
  MarkAttribute.Create.Free;
end.

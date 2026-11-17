program GenericInstanceErrors;

// The checks where type arguments are written (generics.md 2.3, 3.1, 3.6, 4.2): the number of
// type arguments and a name that is not generic or needs type arguments (E0761), a type
// argument that does not satisfy a constraint (E0759, with the notes of `constructor`), a
// reference counted class or an interface for `NotRefCounted` (E0760, also beside `class`), and
// `TArray<Integer>.Create` (E0700 with its notes). A generic type qualified with a name that is
// not a unit is an undeclared identifier (E0700), as one without type arguments is.

type
  TBox<T> = class
    Item: T;
  end;
  TMake<T: class, constructor> = class
  end;
  TOwned<T: class, NotRefCounted> = class
  end;
  TKeyed<K: NotRefCounted> = record
    Key: K;
  end;
  TValue<T: record> = record
    V: T;
  end;
  TShape = class
  end;
  TCircle = class(TShape)
  end;
  TOnly<T: TShape> = class
  end;
  IThing = interface
  end;
  TThing = class(TRefObject, IThing)
  end;
  TNamed<T: IThing> = class
  end;
  TAbstract = class abstract
  end;
  TNeedsArg = class
    constructor Create(X: Integer);
  end;
  TDefaults = class
    constructor Create(X: Integer = 0);
  end;
  TPlain = class
  end;

constructor TNeedsArg.Create(X: Integer);
begin
end;

constructor TDefaults.Create(X: Integer);
begin
end;

var
  A: TBox;
  B: TBox<Integer, string>;
  C: Integer<string>;
  M1: TMake<TPlain>;
  M2: TMake<TAbstract>;
  M3: TMake<TNeedsArg>;
  M4: TMake<TDefaults>;
  M5: TMake<Integer>;
  O1: TOwned<TThing>;
  O2: TOwned<TPlain>;
  O3: TOwned<TObject>;
  O4: TOwned<IThing>;
  K1: TKeyed<IThing>;
  K2: TKeyed<string>;
  V1: TValue<string>;
  V2: TValue<Double>;
  V3: TValue<TBox<Integer>>;
  S1: TOnly<TCircle>;
  S2: TOnly<TPlain>;
  N1: TNamed<TThing>;
  N2: TNamed<IThing>;
  N3: TNamed<TPlain>;
  U: Nope.TBox<Integer>;
  X: Integer;
begin
  X := TBox.Create.Item;
  X := Length(TArray<Integer>.Create(1, 2));
end.

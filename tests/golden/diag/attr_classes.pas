program AttrClasses;

// The checks of an attribute class where it is written (attributes.md 4.2): E0742 for an
// abstract class, the constructor chosen by the arguments among all the constructors whatever
// their names (E0705, E0702 and E0735 as for `TFoo.Create(…)`), E0743 when the declaration
// cannot see the constructor, E0713 for an argument that is neither a constant nor a class name,
// and E0705 for a language attribute or the built-in `TObject.Create` given arguments. A
// parameter's attributes written on both the declaration and the body must be the same (E0723):
// they are compared attribute by attribute (`[A, B]` and `[A][B]` are the same, as are `[A]` and
// `[A()]`), with names in any case but literals as written (attributes.md 11.4).

type
  ShapeAttribute = class abstract(TCustomAttribute)
  end;
  NameAttribute = class(TCustomAttribute)
  public
    constructor Create(const S: string);
  end;
  RangeAttribute = class(TCustomAttribute)
  public
    constructor Make(Low, High: Integer);
    constructor FromText(const S: string; N: Integer);
  end;
  HiddenAttribute = class(TCustomAttribute)
  strict private
    constructor Create;
  end;
  MarkAttribute = class(TCustomAttribute)
  end;

constructor NameAttribute.Create(const S: string);
begin
end;

constructor RangeAttribute.Make(Low, High: Integer);
begin
end;

constructor RangeAttribute.FromText(const S: string; N: Integer);
begin
end;

constructor HiddenAttribute.Create;
begin
end;

var
  Title: string;

type
  [Shape] TA = class
  end;
  [Name(1)] TB = class
  end;
  [Name] TC = class
  end;
  [Range(True)] TD = class
  end;
  [Name(Title)] TE = class
  end;
  [Mark(1)] TF = class
  end;
  [Volatile(1)] TG = class
  end;
  [Range(1, 2), Range('a', 3), Name('ok')] TH = class
  end;
  [Hidden] TI = class
  end;

procedure Show(const [Name('a')] S: string); forward;

procedure Show(const [Name('b')] S: string);
begin
  WriteLn(S);
end;

procedure Lists(const [Name('a'), Mark] S: string; [Mark] N: Integer); forward;

procedure Lists(const [name('a')][Mark] S: string; [Mark()] N: Integer);
begin
  WriteLn(S, N);
end;

procedure Spelled(const [Name('A')] S: string); forward;

procedure Spelled(const [Name('a')] S: string);
begin
  WriteLn(S);
end;

begin
  Title := 'x';
  Lists(Title, 1);
  Spelled(Title);
  Show(Title);
end.

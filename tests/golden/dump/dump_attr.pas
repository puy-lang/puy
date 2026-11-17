program DumpAttr;

// The `.attr` lines of the `hir` dump (attributes.md 7): under the line of each declaration an
// attribute class is written on (a type, a field, a method, a property, a `class var`, a global
// variable, a routine, a parameter, a local variable), with the constructor chosen (`fN`, or
// `builtin` for the inherited `TObject.Create`) and the arguments as expressions (a constant, a
// class name). A declaration of several names has the attribute on each. Language attributes
// and ignored ones are not shown.

type
  NameAttribute = class(TCustomAttribute)
  public
    constructor Create(const S: string; N: Integer = 0);
  end;
  KindAttribute = class(TCustomAttribute)
  public
    constructor ForClass(C: TClass);
  end;
  MarkAttribute = class(TCustomAttribute)
  end;

  [Name('point', 2), Mark]
  TPoint = class(TRefObject)
  public
    [Kind(TPoint)] X, Y: Integer;
    class var [Mark] Count: Integer;
    [Mark] procedure Move;
    [Name('len')] property Len: Integer read X;
  end;

constructor NameAttribute.Create(const S: string; N: Integer);
begin
end;

constructor KindAttribute.ForClass(C: TClass);
begin
end;

procedure TPoint.Move;
begin
  Inc(X);
end;

[Mark]
procedure Show([Name('p')] const P: TPoint);
var
  [Mark] I: Integer;
begin
  I := P.X;
  WriteLn(I);
end;

var
  [Mark, Ref] G: TPoint;
begin
  G := TPoint.Create;
  G.Move;
  Show(G);
end.

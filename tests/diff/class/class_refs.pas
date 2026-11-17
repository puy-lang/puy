program ClassRefs;

// Class references and the members of classes (objects.md 2.2, 2.3, 2.8, 3.1, chapter 5): a
// virtual constructor called through a `class of` variable creates the child, and a constructor
// that is not virtual runs the static type's; class methods see the class reference as `Self`
// (also when called through an instance), virtual class methods are chosen by the class the
// reference points to, and `inherited` keeps `Self`; static methods; a `class var` is shared with
// the descendants; `ClassType`, `ClassParent`, `InheritsFrom`, `ClassNameIs`; class constants and
// nested types used with and without the class's name; comparing class references.

type
  TShape = class
  public
    type
      TKind = (Square, Circle);
      TSize = record
        W, H: Integer;
      end;
    const
      MaxSides = 8;
      Title: string = 'shape';
    class var Count: Integer;
    class var Log: string;
    constructor Create; virtual;
    function Sides: Integer; virtual;
    class function Describe: string; virtual;
    class function Twice(N: Integer): Integer; static;
    function Kind: TKind; virtual;
    function Size: TSize;
  end;
  TSquare = class(TShape)
    constructor Create; override;
    function Sides: Integer; override;
    class function Describe: string; override;
    function Kind: TShape.TKind; override;
  end;
  TCube = class(TSquare)
  end;
  TPlain = class
    Name: string;
    constructor Create;
  end;
  TPlainChild = class(TPlain)
    constructor Create;
  end;
  TShapeClass = class of TShape;
  TPlainClass = class of TPlain;

constructor TShape.Create;
begin
  Inc(Count);
  Log := Log + 'S';
end;

function TShape.Sides: Integer;
begin
  Result := 0;
end;

class function TShape.Describe: string;
begin
  Result := Title + ' ' + ClassName;
end;

class function TShape.Twice(N: Integer): Integer;
begin
  Result := N * 2 + MaxSides;
end;

function TShape.Kind: TKind;
begin
  Result := TKind.Circle;
end;

function TShape.Size: TSize;
begin
  Result := Default(TSize);
  Result.W := MaxSides;
  Result.H := Sides;
end;

constructor TSquare.Create;
begin
  inherited Create;
  Log := Log + 'Q';
end;

function TSquare.Sides: Integer;
begin
  Result := 4;
end;

class function TSquare.Describe: string;
begin
  Result := 'square / ' + inherited Describe;
end;

function TSquare.Kind: TShape.TKind;
begin
  Result := TShape.TKind.Square;
end;

constructor TPlain.Create;
begin
  Name := 'plain';
end;

constructor TPlainChild.Create;
begin
  Name := 'child';
end;

procedure Show(C: TShapeClass);
var
  S: TShape;
begin
  S := C.Create;
  WriteLn(S.ClassName, ' ', S.Sides, ' ', Ord(S.Kind), ' ', C.Describe, ' ', S.Describe);
  S.Free;
end;

var
  C: TShapeClass;
  K: TClass;
  P: TPlainClass;
  S: TShape;
  Q: TPlain;
  Z: TShape.TSize;
begin
  Show(TShape);
  Show(TSquare);
  Show(TCube);
  WriteLn(TShape.Count, ' ', TSquare.Count, ' ', TShape.Log);
  TCube.Count := 10;
  WriteLn(TShape.Count);

  // A constructor that is not virtual runs the static type's
  P := TPlainChild;
  Q := P.Create;
  WriteLn(Q.ClassName, ' ', Q.Name);
  Q.Free;

  WriteLn(TShape.Twice(3), ' ', TSquare.Twice(1));
  C := TCube;
  S := C.Create;
  K := S.ClassType;
  WriteLn(K.ClassName, ' ', K.ClassParent.ClassName, ' ', S.ClassParent.ClassName);
  WriteLn(K.InheritsFrom(TShape), ' ', TShape.InheritsFrom(K), ' ', S.InheritsFrom(TSquare),
    ' ', TCube.InheritsFrom(TCube));
  WriteLn(S.ClassNameIs('tcube'), ' ', TCube.ClassNameIs('TSquare'));
  WriteLn(C = TCube, ' ', C <> TShape, ' ', C = nil, ' ', Assigned(C), ' ', TObject.ClassParent = nil);
  C := nil;
  WriteLn(Assigned(C));
  Z := S.Size;
  WriteLn(Z.W, ' ', Z.H, ' ', TShape.MaxSides, ' ', TShape.Title, ' ', Ord(TShape.TKind.Circle));
  S.Free;
end.

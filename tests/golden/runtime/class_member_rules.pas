program ClassMemberRules;

// Rules of class members without a case elsewhere (objects.md 1, 2.3, 2.4, 2.5, 2.9, 2.10,
// chapter 5): `dynamic` is called like `virtual`; the body of a nested class's method is written
// `TOuter.TInner.Run`; a body may leave out the parameters of a method that is not `overload`;
// inside a method a field hides a global and a local hides a field, and a routine nested in a
// method sees its own locals, then the method's, then the members, then the globals; through a
// `nil` instance a class method is EAccessViolation, while a static method, a `class var` and a
// class constant do not look at the instance; a class method called from a static method gets
// the class the static method belongs to; `as` and the conversion to an ancestor do not look at a
// freed instance; `strict protected` members are used in a descendant's method and `published` is
// `public`; `Equals` without `override` is an ordinary method (W0021: it hides `TObject.Equals`);
// `ClassName` of a nested class is not qualified; `InheritsFrom(nil)` is False; a child's `Create`
// with other parameters hides `TObject.Create` without W0021; the same language attribute twice is
// accepted.

uses
  System.SysUtils;

var
  Count: Integer;

type
  TBase = class
  strict protected
    Secret: Integer;
  published
    Shown: Integer;
  public
    procedure Speak; dynamic;
    class procedure Who;
    class function Twice(X: Integer): Integer; static;
    class function Make: Integer; static;
    function Equals(Other: TBase): Boolean;
  const
    MaxSides = 8;
  class var
    Total: Integer;
  end;

  TChild = class(TBase)
  public
    constructor Create(X: Integer);
    procedure Speak; override;
    procedure Show(Y: Integer);
    function Reveal: Integer;
  end;

  TOuter = class
  type
    TInner = class
      procedure Run;
    end;
  end;

  TNode = class(TRefObject)
  end;

  THolder = class
    [weak, weak] Link: TNode;
  end;

procedure TBase.Speak;
begin
  WriteLn('base speaks');
end;

class procedure TBase.Who;
begin
  WriteLn('who: ', ClassName);
end;

class function TBase.Twice(X: Integer): Integer;
begin
  Result := X * 2;
end;

class function TBase.Make: Integer;
begin
  Who;
  Result := 0;
end;

function TBase.Equals(Other: TBase): Boolean;
begin
  Result := Other = Self;
end;

constructor TChild.Create(X: Integer);
begin
  inherited Create;
  Secret := X;
end;

procedure TChild.Speak;
begin
  WriteLn('child speaks');
end;

procedure TChild.Show;
var
  Count: Integer;

  procedure Inner;
  var
    Shown: Integer;
  begin
    Shown := 100;
    WriteLn('inner: ', Shown, ' ', Count, ' ', Secret);
  end;

begin
  Count := Y;
  WriteLn('show: ', Count, ' ', Shown, ' ', Secret);
  Inner;
end;

function TChild.Reveal: Integer;
begin
  Result := Secret;
end;

procedure TOuter.TInner.Run;
begin
  WriteLn('inner run: ', ClassName);
end;

var
  B: TBase;
  C: TChild;
  N: TBase;
  I: TOuter.TInner;
  F: TObject;
  H: THolder;
begin
  Count := -1;
  B := TChild.Create(7);
  B.Speak;
  C := TChild(B);
  C.Shown := 3;
  C.Show(42);
  WriteLn('reveal: ', C.Reveal, ' global: ', Count);
  I := TOuter.TInner.Create;
  I.Run;
  WriteLn('nested name: ', I.ClassName);
  I.Free;

  N := nil;
  WriteLn('static: ', N.Twice(4), ' const: ', N.MaxSides);
  N.Total := 5;
  WriteLn('class var: ', TBase.Total);
  try
    N.Who;
  except
    on E: EAccessViolation do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  TChild.Make;

  F := TObject.Create;
  F.Free;
  WriteLn('as ancestor: ', Assigned(F as TObject), ' ', Assigned(TObject(F)));

  WriteLn('equals: ', B.Equals(C), ' ', B.Equals(nil));
  WriteLn('inherits nil: ', B.InheritsFrom(nil));
  H := THolder.Create;
  WriteLn('weak twice: ', H.Link = nil);
  H.Free;
  B.Free;
end.

program GenericConstraintMembers;

uses
  System.SysUtils;

// A generic body uses its type parameters' values through their constraints (generics.md 3.3,
// 3.4, 4.3): the class type constraint's virtual methods are called through the table, and its
// non-virtual methods are the constraint's even when the concrete class hides them with methods
// of the same name. An interface constraint's methods are called on a class (the method the
// class maps to the interface's) and on an interface. `nil`, `Assigned` and `=` / `<>` compare
// the references (a class's `Equal` operator is not used), `is` / `as` / `Supports` see the
// value as its constraint, and a value converts implicitly to the constraint's class and
// interfaces.

type
  TShape = class
    function Area: Double; virtual; abstract;
    function Name: string;
  end;

  TSquare = class(TShape)
    FSide: Double;
    constructor Create(ASide: Double);
    function Area: Double; override;
    function Name: string;
  end;

  TRect = class(TShape)
    FW, FH: Double;
    constructor Create(AW, AH: Double);
    function Area: Double; override;
  end;

  TShapes<T: TShape> = class
    FItems: TArray<T>;
    function TotalArea: Double;
    procedure ShowNames;
    function Largest: TShape;
  end;

  IGreet = interface
    procedure Hello;
  end;

  ICount = interface
    function Count: Integer;
  end;

  ILoud = interface(IGreet)
    procedure Shout;
  end;

  TNode = class(TRefObject, IGreet, ICount)
    FName: string;
    constructor Create(const AName: string);
    procedure Hello;
    procedure Greet;
    function Count: Integer;
    class operator Equal(const A, B: TNode): Boolean;
    function GetHashCode: Int64; override;
  end;

  TLoud = class(TRefObject, ILoud)
    procedure Hello;
    procedure Shout;
  end;

  TGreeters<T: IGreet, ICount> = class
    procedure Run(const X: T);
  end;

  TCallers<T: IGreet> = class
    procedure Run(const X: T);
  end;

  TKeep<T: class> = class
    FItem: T;
    function Same(const A, B: T): Boolean;
    function Has: Boolean;
    procedure Clear;
    function AsObject: TObject;
    procedure Inspect;
  end;

function TShape.Name: string;
begin
  Result := 'shape';
end;

constructor TSquare.Create(ASide: Double);
begin
  FSide := ASide;
end;

function TSquare.Area: Double;
begin
  Result := FSide * FSide;
end;

function TSquare.Name: string;
begin
  Result := 'square';
end;

constructor TRect.Create(AW, AH: Double);
begin
  FW := AW;
  FH := AH;
end;

function TRect.Area: Double;
begin
  Result := FW * FH;
end;

function TShapes<T>.TotalArea: Double;
begin
  Result := 0;
  for var S in FItems do
    Result := Result + S.Area; // TShape.Area (virtual) through the class type constraint
end;

procedure TShapes<T>.ShowNames;
begin
  for var S in FItems do
    WriteLn(S.Name); // TShape.Name, not TSquare.Name
end;

function TShapes<T>.Largest: TShape;
begin
  Result := nil;
  for var S in FItems do
    if (Result = nil) or (S.Area > Result.Area) then
      Result := S;
end;

constructor TNode.Create(const AName: string);
begin
  FName := AName;
end;

procedure TNode.Hello;
begin
  WriteLn('hello from ', FName);
end;

procedure TNode.Greet;
begin
  Hello;
end;

function TNode.Count: Integer;
begin
  Result := Length(FName);
end;

class operator TNode.Equal(const A, B: TNode): Boolean;
begin
  Result := True;
end;

function TNode.GetHashCode: Int64;
begin
  Result := 0;
end;

procedure TLoud.Hello;
begin
  WriteLn('hello, loudly');
end;

procedure TLoud.Shout;
begin
  WriteLn('HELLO');
end;

procedure TGreeters<T>.Run(const X: T);
var
  G: IGreet;
begin
  X.Hello;
  WriteLn(X.Count);
  G := X;
  G.Hello;
end;

procedure TCallers<T>.Run(const X: T);
begin
  X.Hello;
  WriteLn(Assigned(X));
end;

function TKeep<T>.Same(const A, B: T): Boolean;
begin
  Result := A = B;
end;

function TKeep<T>.Has: Boolean;
begin
  Result := Assigned(FItem) and (FItem <> nil);
end;

procedure TKeep<T>.Clear;
begin
  FItem := nil;
end;

function TKeep<T>.AsObject: TObject;
begin
  Result := FItem;
end;

procedure TKeep<T>.Inspect;
var
  G: IGreet;
begin
  if FItem is TNode then
    WriteLn('a node named ', (FItem as TNode).FName)
  else
    WriteLn('not a node');
  if Supports(FItem, IGreet, G) then
    G.Hello
  else
    WriteLn('no greeting');
end;

var
  Shapes: TShapes<TSquare>;
  Mixed: TShapes<TShape>;
  Greeters: TGreeters<TNode>;
  Callers: TCallers<IGreet>;
  LoudCallers: TCallers<ILoud>;
  Keep: TKeep<TNode>;
  Objects: TKeep<TObject>;
  A, B: TNode;
begin
  Shapes := TShapes<TSquare>.Create;
  Shapes.FItems := [TSquare.Create(2), TSquare.Create(3)];
  WriteLn(Shapes.TotalArea:0:1);
  Shapes.ShowNames;
  Mixed := TShapes<TShape>.Create;
  Mixed.FItems := [TSquare.Create(1), TRect.Create(2, 5), TSquare.Create(3)];
  WriteLn(Mixed.TotalArea:0:1, ' ', Mixed.Largest.ClassName);

  A := TNode.Create('ab');
  B := TNode.Create('cde');
  Greeters := TGreeters<TNode>.Create;
  Greeters.Run(B);
  Callers := TCallers<IGreet>.Create;
  Callers.Run(A);
  LoudCallers := TCallers<ILoud>.Create;
  LoudCallers.Run(TLoud.Create);

  Keep := TKeep<TNode>.Create;
  WriteLn(A = B, ' ', Keep.Same(A, B), ' ', Keep.Same(A, A));
  WriteLn(Keep.Has);
  Keep.FItem := A;
  WriteLn(Keep.Has, ' ', Keep.AsObject.ClassName);
  Keep.Inspect;
  Keep.Clear;
  WriteLn(Keep.Has);
  Objects := TKeep<TObject>.Create;
  Objects.FItem := Mixed;
  Objects.Inspect;
  Objects.FItem := B;
  Objects.Inspect;
  Objects.Clear;

  for var S in Shapes.FItems do
    S.Free;
  for var S in Mixed.FItems do
    S.Free;
  Shapes.Free;
  Mixed.Free;
  Greeters.Free;
  Callers.Free;
  LoudCallers.Free;
  Keep.Free;
  Objects.Free;
end.

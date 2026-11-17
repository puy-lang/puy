program Shapes;

uses
  System.SysUtils;

// Classes: inheritance, virtual and abstract methods, constructors and
// destructors, properties, class references (class of) and is / as.
// TShape descends from TObject, so instances are freed by hand with Free.

type
  EShapeError = class(Exception);

  TShape = class abstract
  private
    FName: string;
    class var FCount: Integer;
  public
    constructor Create(const AName: string); virtual;
    destructor Destroy; override;
    function Area: Double; virtual; abstract;
    function Describe: string; virtual;
    property Name: string read FName;
    class property Count: Integer read FCount;
  end;

  TShapeClass = class of TShape;

  TCircle = class(TShape)
  private
    FRadius: Double;
  public
    function Area: Double; override;
    function Describe: string; override;
    property Radius: Double read FRadius write FRadius;
  end;

  TRectangle = class(TShape)
  private
    FWidth, FHeight: Double;
    procedure SetWidth(Value: Double);
  public
    constructor Create(const AName: string); override;
    function Area: Double; override;
    property Width: Double read FWidth write SetWidth;
    property Height: Double read FHeight write FHeight;
  end;

  TSquare = class sealed(TRectangle)
  public
    function Describe: string; override;
  end;

constructor TShape.Create(const AName: string);
begin
  inherited Create;
  FName := AName;
  Inc(FCount);
end;

destructor TShape.Destroy;
begin
  Dec(FCount);
  WriteLn('  freed ', FName);
  inherited;
end;

function TShape.Describe: string;
begin
  Result := Format('%s (%s) area %.2f', [FName, ClassName, Area]);
end;

function TCircle.Area: Double;
begin
  Result := Pi * Sqr(FRadius);
end;

function TCircle.Describe: string;
begin
  Result := inherited Describe + Format(', radius %.1f', [FRadius]);
end;

constructor TRectangle.Create(const AName: string);
begin
  inherited Create(AName);
  FWidth := 1;
  FHeight := 1;
end;

procedure TRectangle.SetWidth(Value: Double);
begin
  if Value <= 0 then
    raise EShapeError.CreateFmt('%s: width must be positive', [Name]);
  FWidth := Value;
end;

function TRectangle.Area: Double;
begin
  Result := FWidth * FHeight;
end;

function TSquare.Describe: string;
begin
  Result := 'a square: ' + inherited Describe;
end;

// A class reference picks the class at run time; Create is virtual, so
// TRectangle's constructor runs for TRectangle and TSquare.
function Make(Kind: TShapeClass; const Name: string): TShape;
begin
  Result := Kind.Create(Name);
end;

var
  Shapes: TArray<TShape>;

begin
  Shapes := [Make(TCircle, 'wheel'), Make(TRectangle, 'door'), Make(TSquare, 'tile')];
  WriteLn(TShape.Count, ' shapes');
  try
    // is tests the class, a cast with the type name converts after a test.
    for var S in Shapes do
      if S is TCircle then
        TCircle(S).Radius := 2
      else if S is TRectangle then
        TRectangle(S).Height := 3;

    // as converts and raises EInvalidCast when the class does not match.
    (Shapes[2] as TRectangle).Height := 2;
    (Shapes[2] as TRectangle).Width := 2;

    for var S in Shapes do
      WriteLn(S.Describe);

    try
      (Shapes[1] as TRectangle).Width := 0;
    except
      on E: EShapeError do
        WriteLn('refused: ', E.Message);
    end;

    try
      (Shapes[0] as TRectangle).Width := 5;
    except
      on E: EInvalidCast do
        WriteLn('not a rectangle: ', Shapes[0].ClassName);
    end;

    WriteLn('TSquare inherits from TRectangle: ', TSquare.InheritsFrom(TRectangle));
    WriteLn('parent of TSquare: ', TSquare.ClassParent.ClassName);
  finally
    for var S in Shapes do
      S.Free;
  end;
  WriteLn(TShape.Count, ' shapes left');
end.

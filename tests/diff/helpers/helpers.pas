program Helpers;

// Class helpers and record helpers (objects.md 10.1-10.8), one helper per type: a class helper's
// methods see the public members of the class and are called on instances of the class and of
// its descendants; a class method of the helper is called through the class; a record helper
// for a record, a string, an integer type and an enumeration adds methods and properties, its
// methods change the variable they are called on through `Self`, and a value that is not a
// variable (a constant, an expression, a function's result) is copied first.

type
  TColor = (Red, Green, Blue);

  TAnimal = class
  public
    Name: string;
    function Sound: string; virtual;
  end;

  TDog = class(TAnimal)
  public
    function Sound: string; override;
  end;

  TAnimalHelper = class helper for TAnimal
  public
    function Describe: string;
    procedure Rename(const S: string);
    class function Kind: string;
  end;

  TPair = record
    A, B: Integer;
  end;

  TPairHelper = record helper for TPair
  private
    function GetSum: Integer;
  public
    procedure Swap;
    function Max: Integer;
    property Sum: Integer read GetSum;
  end;

  TTextHelper = record helper for string
    function Twice: string;
    procedure Append(const S: string);
    function Count(C: Char): Integer;
  end;

  TIntHelper = record helper for Integer
    function Squared: Integer;
    procedure Bump;
    function IsEven: Boolean;
  end;

  TColorHelper = record helper for TColor
    function Next: TColor;
    function Code: Integer;
  end;

function TAnimal.Sound: string;
begin
  Result := '...';
end;

function TDog.Sound: string;
begin
  Result := 'woof';
end;

function TAnimalHelper.Describe: string;
begin
  Result := Name + ' says ' + Sound;
end;

procedure TAnimalHelper.Rename(const S: string);
begin
  Name := S;
end;

class function TAnimalHelper.Kind: string;
begin
  Result := 'animal';
end;

function TPairHelper.GetSum: Integer;
begin
  Result := A + B;
end;

procedure TPairHelper.Swap;
var
  T: Integer;
begin
  T := A;
  A := B;
  B := T;
end;

function TPairHelper.Max: Integer;
begin
  if A > B then
    Result := A
  else
    Result := B;
end;

function TTextHelper.Twice: string;
begin
  Result := Self + Self;
end;

procedure TTextHelper.Append(const S: string);
begin
  Self := Self + S;
end;

function TTextHelper.Count(C: Char): Integer;
var
  I: Integer;
begin
  Result := 0;
  for I := 1 to Length(Self) do
    if Self[I] = C then
      Inc(Result);
end;

function TIntHelper.Squared: Integer;
begin
  Result := Self * Self;
end;

procedure TIntHelper.Bump;
begin
  Self := Self + 1;
end;

function TIntHelper.IsEven: Boolean;
begin
  Result := Self mod 2 = 0;
end;

function TColorHelper.Next: TColor;
begin
  if Self = TColor.Blue then
    Result := TColor.Red
  else
    Result := Succ(Self);
end;

function TColorHelper.Code: Integer;
begin
  Result := Ord(Self) * 10;
end;

function MakePair: TPair;
begin
  Result := Default(TPair);
  Result.A := 7;
  Result.B := 2;
end;

function Three: Integer;
begin
  Result := 3;
end;

var
  Animal: TAnimal;
  P: TPair;
  S: string;
  N: Integer;
  C: TColor;
begin
  Animal := TDog.Create;
  Animal.Name := 'Rex';
  WriteLn(Animal.Describe);
  Animal.Rename('Max');
  WriteLn(Animal.Describe);
  WriteLn(TAnimal.Kind, ' ', TDog.Kind, ' ', Animal.Kind);
  Animal.Free;

  P.A := 3;
  P.B := 5;
  WriteLn(P.Sum, ' ', P.Max);
  P.Swap;
  WriteLn(P.A, ' ', P.B);
  WriteLn(MakePair.Max, ' ', MakePair.Sum);

  S := 'ab';
  WriteLn(S.Twice);
  S.Append('c');
  S.Append('a');
  WriteLn(S, ' ', S.Count('a'));
  WriteLn('xyz'.Twice);

  N := 6;
  WriteLn(N.Squared, ' ', N.IsEven);
  N.Bump;
  WriteLn(N, ' ', N.IsEven, ' ', Three.Squared);

  C := TColor.Green;
  WriteLn(C.Code, ' ', Ord(C.Next), ' ', Ord(C.Next.Next));
end.

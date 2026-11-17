program RecordMethodStrings;

// Records with string fields used through methods (objects.md chapter 6): the value a constructor
// runs on, the copies a method on a `const` parameter, a function's result or a loop variable
// writes to, `Self` passed on from a nested routine and from another method, and a property with
// a setter are all let go of: nothing leaks.

type
  TName = record
  private
    FName: string;
    procedure SetName(const V: string);
  public
    class var Count: Integer;
    const Prefix = 'N:';
    constructor Create(const AName: string);
    function Show: string;
    procedure Twice;
    procedure Bump;
    property Name: string read FName write SetName;
    class function Make(const S: string): TName; static;
  end;

constructor TName.Create(const AName: string);
begin
  FName := AName;
  Inc(Count);
end;

procedure TName.SetName(const V: string);
begin
  FName := V + '!';
end;

function TName.Show: string;
begin
  Result := Prefix + FName;
end;

procedure TName.Twice;

  procedure Inner;
  begin
    FName := FName + FName;
    Self.FName := Self.FName + '.';
  end;

begin
  Inner;
  Bump;
end;

procedure TName.Bump;
begin
  FName := FName + '+';
end;

class function TName.Make(const S: string): TName;
begin
  Result := TName.Create(S);
end;

function Get: TName;
begin
  Result := TName.Create('g');
end;

procedure ShowConst(const N: TName);
begin
  N.Bump;
  WriteLn(N.Show);
end;

var
  N: TName;
  Arr: array of TName;
  E: TName;
  I: Integer;
begin
  N := TName.Make('ab');
  N.Twice;
  WriteLn(N.Show);
  N.Name := 'x';
  WriteLn(N.Name);
  ShowConst(N);
  WriteLn(TName.Count, ' ', TName.Prefix);
  Arr := [TName.Create('a'), TName.Create('b')];
  for E in Arr do
    E.Bump;
  WriteLn(Arr[0].Show, Arr[1].Show);
  for I := 0 to High(Arr) do
    Arr[I].Bump;
  WriteLn(Arr[0].Show, Arr[1].Show);
  Get.Bump;
  WriteLn(Get.Show, ' ', TName.Count);
end.

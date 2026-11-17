program Collections;

uses
  System.SysUtils;

// A small list class: a default array property (List[I], overloaded by the
// type of the index), slices through TIndexRange (List[2..4]), and for..in
// through GetEnumerator. The list is reference counted, so it is freed when
// the last reference goes away.

type
  ENameError = class(Exception);

  TScoreList = class;

  // A record enumerator: for..in calls MoveNext and reads Current.
  TScoreEnumerator = record
  private
    FList: TScoreList;
    FIndex: Integer;
  public
    constructor Create(AList: TScoreList);
    function MoveNext: Boolean;
    function GetCurrent: Integer;
    property Current: Integer read GetCurrent;
  end;

  TScoreList = class(TRefObject)
  private
    FNames: TArray<string>;
    FScores: TArray<Integer>;
    function GetCount: Integer;
    function GetScore(Index: Integer): Integer;
    procedure SetScore(Index: Integer; Value: Integer);
    function GetByName(const Name: string): Integer;
    procedure SetByName(const Name: string; Value: Integer);
    function GetRange(const R: TIndexRange): TArray<Integer>;
    function IndexOf(const Name: string): Integer;
  public
    procedure Add(const Name: string; Score: Integer);
    function GetEnumerator: TScoreEnumerator;
    function Total: Integer;
    property Count: Integer read GetCount;
    property Scores[Index: Integer]: Integer read GetScore write SetScore; default;
    property Scores[const Name: string]: Integer read GetByName write SetByName; default;
    property Scores[const R: TIndexRange]: TArray<Integer> read GetRange; default;
  end;

  // An enumerator can also be a class; for..in frees it after the loop.
  TCountdown = class
  private
    FCurrent: Integer;
  public
    constructor Create(From: Integer);
    destructor Destroy; override;
    function MoveNext: Boolean;
    property Current: Integer read FCurrent;
  end;

  TLaunch = record
    From: Integer;
    function GetEnumerator: TCountdown;
  end;

constructor TScoreEnumerator.Create(AList: TScoreList);
begin
  FList := AList;
  FIndex := -1;
end;

function TScoreEnumerator.MoveNext: Boolean;
begin
  Inc(FIndex);
  Result := FIndex < FList.Count;
end;

function TScoreEnumerator.GetCurrent: Integer;
begin
  Result := FList[FIndex];
end;

function TScoreList.GetCount: Integer;
begin
  Result := Length(FScores);
end;

function TScoreList.GetScore(Index: Integer): Integer;
begin
  if (Index < 0) or (Index >= Count) then
    raise ERangeError.CreateFmt('index %d is out of range', [Index]);
  Result := FScores[Index];
end;

procedure TScoreList.SetScore(Index: Integer; Value: Integer);
begin
  if (Index < 0) or (Index >= Count) then
    raise ERangeError.CreateFmt('index %d is out of range', [Index]);
  FScores[Index] := Value;
end;

function TScoreList.IndexOf(const Name: string): Integer;
begin
  for var I := 0 to High(FNames) do
    if SameText(FNames[I], Name) then
      Exit(I);
  Result := -1;
end;

function TScoreList.GetByName(const Name: string): Integer;
begin
  var I := IndexOf(Name);
  if I < 0 then
    raise ENameError.CreateFmt('no score for %s', [Name]);
  Result := FScores[I];
end;

// Writing to a name that is not in the list adds it.
procedure TScoreList.SetByName(const Name: string; Value: Integer);
begin
  var I := IndexOf(Name);
  if I < 0 then
    Add(Name, Value)
  else
    FScores[I] := Value;
end;

// Clamp applies the rules of the built-in slices: missing ends are the
// first and last index, and ends outside the list are cut off.
function TScoreList.GetRange(const R: TIndexRange): TArray<Integer>;
var
  First, Last: Integer;
begin
  R.Clamp(0, Count - 1, First, Last);
  Result := Copy(FScores, First, Last - First + 1);
end;

procedure TScoreList.Add(const Name: string; Score: Integer);
begin
  FNames := FNames + [Name];
  FScores := FScores + [Score];
end;

function TScoreList.GetEnumerator: TScoreEnumerator;
begin
  Result := TScoreEnumerator.Create(Self);
end;

function TScoreList.Total: Integer;
begin
  Result := 0;
  for var Score in Self do
    Inc(Result, Score);
end;

constructor TCountdown.Create(From: Integer);
begin
  inherited Create;
  FCurrent := From + 1;
end;

destructor TCountdown.Destroy;
begin
  WriteLn('  (countdown freed)');
  inherited;
end;

function TCountdown.MoveNext: Boolean;
begin
  Dec(FCurrent);
  Result := FCurrent > 0;
end;

function TLaunch.GetEnumerator: TCountdown;
begin
  Result := TCountdown.Create(From);
end;

function Show(const Values: TArray<Integer>): string;
begin
  Result := '';
  for var V in Values do
  begin
    if Result <> '' then
      Result := Result + ', ';
    Result := Result + IntToStr(V);
  end;
  Result := '[' + Result + ']';
end;

var
  List: TScoreList;
  Launch: TLaunch;

begin
  List := TScoreList.Create;
  List.Add('Alice', 82);
  List.Add('Bob', 67);
  List.Add('Carol', 91);
  List.Add('Dave', 74);
  List['Erin'] := 88;

  WriteLn('by index:  List[0] = ', List[0], ', List[2] = ', List[2]);
  WriteLn('by name:   List[''bob''] = ', List['bob']);
  List[1] := List[1] + 10;
  List['Dave'] := 79;
  Write('for..in:  ');
  for var Score in List do
    Write(' ', Score);
  WriteLn;
  WriteLn('total:     ', List.Total, ' over ', List.Count, ' scores');

  WriteLn('slices:    List[1..3] = ', Show(List[1..3]));
  WriteLn('           List[3..] = ', Show(List[3..]), ', List[..1] = ', Show(List[..1]));
  WriteLn('           List[2..100] = ', Show(List[2..100]), ', List[4..2] = ', Show(List[4..2]));

  try
    WriteLn(List['Zoe']);
  except
    on E: ENameError do
      WriteLn('error:     ', E.Message);
  end;
  try
    List[5] := 0;
  except
    on E: ERangeError do
      WriteLn('error:     ', E.Message);
  end;

  WriteLn('countdown:');
  Launch.From := 3;
  for var N in Launch do
    WriteLn('  ', N);
  WriteLn('  liftoff');
end.

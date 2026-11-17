program Equality;

uses
  System.SysUtils, System.Generics.Collections;

// How containers compare keys. A record without an Equal operator is compared field by field,
// strings by content, so it can be a dictionary key as it is. A record that declares Equal also
// declares GetHashCode, and LessThan gives it a default order for Sort. A class is compared by
// identity unless it overrides both Equals and GetHashCode. KeyEquals, KeyHash and KeyCompare
// are the same rules as plain routines, and HashCombine mixes hashes.

type
  // Compared field by field: no operator needed.
  TCell = record
    Row, Col: Integer;
  end;

  // A version number: 1.2 and 1.2.0 are equal, and versions sort by number, not as text.
  TVersion = record
    Parts: TArray<Integer>;
    class function Parse(const S: string): TVersion; static;
    function ToString: string;
    function Part(I: Integer): Integer;
    function GetHashCode: Int64;
    class operator Equal(const A, B: TVersion): Boolean;
    class operator LessThan(const A, B: TVersion): Boolean;
  end;

  // A value object: two instances with the same code are the same currency.
  TCurrencyCode = class(TRefObject)
  private
    FCode: string;
  public
    constructor Create(const ACode: string);
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
    property Code: string read FCode;
  end;

  // No overrides: each instance is a different key.
  TSession = class(TRefObject)
  end;

function Cell(Row, Col: Integer): TCell;
begin
  Result := Default(TCell);
  Result.Row := Row;
  Result.Col := Col;
end;

class function TVersion.Parse(const S: string): TVersion;
var
  Start: Integer;
begin
  Result := Default(TVersion);
  Start := 1;
  for var I := 1 to Length(S) + 1 do
    if (I > Length(S)) or (S[I] = '.') then
    begin
      Result.Parts := Result.Parts + [StrToInt(Copy(S, Start, I - Start))];
      Start := I + 1;
    end;
end;

function TVersion.ToString: string;
begin
  Result := '';
  for var P in Parts do
  begin
    if Result <> '' then
      Result := Result + '.';
    Result := Result + P.ToString;
  end;
end;

// Missing parts count as 0.
function TVersion.Part(I: Integer): Integer;
begin
  if I < Length(Parts) then
    Result := Parts[I]
  else
    Result := 0;
end;

function PartCount(const A, B: TVersion): Integer;
begin
  Result := Length(A.Parts);
  if Length(B.Parts) > Result then
    Result := Length(B.Parts);
end;

// Equal versions must have equal hashes, so trailing zeros are left out of the hash.
function TVersion.GetHashCode: Int64;
var
  Last: Integer;
begin
  Last := High(Parts);
  while (Last >= 0) and (Parts[Last] = 0) do
    Dec(Last);
  Result := 0;
  for var I := 0 to Last do
    Result := HashCombine(Result, KeyHash(Parts[I]));
end;

class operator TVersion.Equal(const A, B: TVersion): Boolean;
begin
  for var I := 0 to PartCount(A, B) - 1 do
    if A.Part(I) <> B.Part(I) then
      Exit(False);
  Result := True;
end;

class operator TVersion.LessThan(const A, B: TVersion): Boolean;
begin
  for var I := 0 to PartCount(A, B) - 1 do
    if A.Part(I) <> B.Part(I) then
      Exit(A.Part(I) < B.Part(I));
  Result := False;
end;

constructor TCurrencyCode.Create(const ACode: string);
begin
  inherited Create;
  FCode := ACode;
end;

function TCurrencyCode.Equals(Obj: TObject): Boolean;
begin
  Result := (Obj is TCurrencyCode) and (TCurrencyCode(Obj).FCode = FCode);
end;

function TCurrencyCode.GetHashCode: Int64;
begin
  Result := KeyHash(FCode);
end;

procedure RecordKeys;
var
  Board: TDictionary<TCell, Char>;
begin
  WriteLn('record keys, field by field:');
  Board := TDictionary<TCell, Char>.Create;
  try
    Board.Add(Cell(0, 0), 'X');
    Board.Add(Cell(1, 1), 'O');
    Board.Add(Cell(0, 2), 'X');
    for var Row := 0 to 2 do
    begin
      Write('  ');
      for var Col := 0 to 2 do
      begin
        var Mark: Char;
        if Board.TryGetValue(Cell(Row, Col), Mark) then
          Write(Mark)
        else
          Write('.');
      end;
      WriteLn;
    end;
    WriteLn('  KeyEquals(Cell(1, 2), Cell(1, 2)) = ', KeyEquals(Cell(1, 2), Cell(1, 2)),
      ', KeyHash equal: ', KeyHash(Cell(1, 2)) = KeyHash(Cell(1, 2)));
  finally
    Board.Free;
  end;
end;

procedure Versions;
var
  Released: THashSet<TVersion>;
  Order: TList<TVersion>;
begin
  WriteLn('a record with Equal, GetHashCode and LessThan:');
  Released := THashSet<TVersion>.Create;
  Order := TList<TVersion>.Create;
  try
    var Tags: TArray<string> := ['1.10', '1.2', '1.9.1', '0.9', '1.2.0', '1.10.0.0'];
    for var S in Tags do
    begin
      var V := TVersion.Parse(S);
      if Released.Add(V) then
        Order.Add(V)
      else
        WriteLn('  ', S, ' is already in the set');
    end;
    Order.Sort;
    Write('  sorted:');
    for var V in Order do
      Write(' ', V.ToString);
    WriteLn;
    WriteLn('  contains 1.9.1.0: ', Released.Contains(TVersion.Parse('1.9.1.0')),
      ', 1.9 = 1.9.0: ', TVersion.Parse('1.9') = TVersion.Parse('1.9.0'),
      ', KeyCompare(1.9, 1.10) = ', KeyCompare(TVersion.Parse('1.9'), TVersion.Parse('1.10')));
  finally
    Released.Free;
    Order.Free;
  end;
end;

procedure ClassKeys;
var
  Rates: TDictionary<TCurrencyCode, Double>;
  Sessions: THashSet<TSession>;
  A, B: TSession;
begin
  WriteLn('class keys:');
  Rates := TDictionary<TCurrencyCode, Double>.Create;
  try
    Rates.Add(TCurrencyCode.Create('EUR'), 1.08);
    Rates.Add(TCurrencyCode.Create('JPY'), 0.0067);
    // A new instance with the same code finds the entry, because Equals is overridden.
    WriteLn('  EUR rate: ', Rates[TCurrencyCode.Create('EUR')]:0:2);
    WriteLn('  TryAdd(another EUR) = ', Rates.TryAdd(TCurrencyCode.Create('EUR'), 1.1));
  finally
    Rates.Free;
  end;

  Sessions := THashSet<TSession>.Create;
  try
    A := TSession.Create;
    B := TSession.Create;
    Sessions.Add(A);
    Sessions.Add(B);
    Sessions.Add(A);
    WriteLn('  sessions (by identity): ', Sessions.Count, ', contains a new one: ',
      Sessions.Contains(TSession.Create));
  finally
    Sessions.Free;
  end;
end;

begin
  RecordKeys;
  Versions;
  ClassKeys;
end.

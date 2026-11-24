unit System.RegularExpressions;

// Regular expressions (docs/spec/stdlib/System.RegularExpressions/README.md): TRegEx and its
// matches, groups and collections. The engine, the mapping of its positions to the string's
// code units and the expansion of a replacement string are the Rust parts in
// System.RegularExpressions.Native (crates/puy-stdlib-ext; docs/design/stdlib.md 3.11); the
// API, the checks of the positions, Split and the replacement by an evaluator are here.

interface

uses
  // The engine's classes are the fields of the records below
  System.RegularExpressions.Native;

type
  // An invalid pattern, a position out of the string, a group or a match that is not there.
  ERegularExpressionError = System.RegularExpressions.Native.ERegularExpressionError;

  TRegExOption = (roNone, roIgnoreCase, roMultiLine, roExplicitCapture, roCompiled,
    roSingleLine, roIgnorePatternSpace, roNotEmpty);
  TRegExOptions = set of TRegExOption;

  // A group of a match: Index is 1-based in UTF-16 code units, like the string's.
  TGroup = record
  private
    FInput: string;
    FIndex: Integer;
    FLength: Integer;
    FSuccess: Boolean;
    function GetValue: string;
  public
    property Index: Integer read FIndex;
    property Length: Integer read FLength;
    property Success: Boolean read FSuccess;
    property Value: string read GetValue;
  end;

  TGroupCollectionEnumerator = record
  private
    FInput: string;
    FSpans: TArray<Integer>;
    FIndex: Integer;
    function GetCurrent: TGroup;
  public
    function MoveNext: Boolean;
    property Current: TGroup read GetCurrent;
  end;

  // The groups of a match, the whole match (group 0) first.
  TGroupCollection = record
  private
    FInput: string;
    // The 0-based start (-1 for a group that took no part) and the length of each group
    FSpans: TArray<Integer>;
    FEngine: TRegexEngine;
    function GetCount: Integer;
    function GetItem(Index: Integer): TGroup;
    function GetNamedItem(const Name: string): TGroup;
  public
    function GetEnumerator: TGroupCollectionEnumerator;
    property Count: Integer read GetCount;
    property Item[Index: Integer]: TGroup read GetItem; default;
    property Item[const Name: string]: TGroup read GetNamedItem; default;
  end;

  // A match, or no match (Success is False).
  TMatch = record
  private
    FGroups: TGroupCollection;
    // The text searched, for NextMatch and Result (nil when there is no match)
    FSubject: TRegexSubject;
    function GetIndex: Integer;
    function GetLength: Integer;
    function GetSuccess: Boolean;
    function GetValue: string;
  public
    function NextMatch: TMatch;
    function &Result(const Pattern: string): string;
    property Groups: TGroupCollection read FGroups;
    property Index: Integer read GetIndex;
    property Length: Integer read GetLength;
    property Success: Boolean read GetSuccess;
    property Value: string read GetValue;
  end;

  TMatchCollectionEnumerator = record
  private
    FInput: string;
    FSpans: TArray<Integer>;
    FEngine: TRegexEngine;
    FSubject: TRegexSubject;
    FIndex: Integer;
    function GetCurrent: TMatch;
  public
    function MoveNext: Boolean;
    property Current: TMatch read GetCurrent;
  end;

  // The matches of a search, all found when the collection is made.
  TMatchCollection = record
  private
    FInput: string;
    // The groups' spans of each match, one match after the other
    FSpans: TArray<Integer>;
    FEngine: TRegexEngine;
    FSubject: TRegexSubject;
    function GetCount: Integer;
    function GetItem(Index: Integer): TMatch;
  public
    function GetEnumerator: TMatchCollectionEnumerator;
    property Count: Integer read GetCount;
    property Item[Index: Integer]: TMatch read GetItem; default;
  end;

  // What replaces a match in TRegEx.Replace.
  TMatchEvaluator = reference to function(const Match: TMatch): string;

  // A compiled regular expression. Copies share the compiled pattern.
  TRegEx = record
  private
    FEngine: TRegexEngine;
    function Search(const Input: string; Start, Stop: Integer): TMatch;
    function FindAll(const Input: string; Start, Limit: Integer): TMatchCollection;
    function ReplaceBy(const Input: string; const Evaluator: TMatchEvaluator;
      Count: Integer): string;
  public
    constructor Create(const Pattern: string;
      Options: TRegExOptions = [TRegExOption.roNotEmpty]);
    function IsMatch(const Input: string): Boolean; overload;
    function IsMatch(const Input: string; StartPos: Integer): Boolean; overload;
    class function IsMatch(const Input, Pattern: string): Boolean; overload; static;
    class function IsMatch(const Input, Pattern: string; Options: TRegExOptions): Boolean;
      overload; static;
    class function Escape(const Str: string; UseWildCards: Boolean = False): string; static;
    function Match(const Input: string): TMatch; overload;
    function Match(const Input: string; StartPos: Integer): TMatch; overload;
    function Match(const Input: string; StartPos, ALength: Integer): TMatch; overload;
    class function Match(const Input, Pattern: string): TMatch; overload; static;
    class function Match(const Input, Pattern: string; Options: TRegExOptions): TMatch;
      overload; static;
    function Matches(const Input: string): TMatchCollection; overload;
    function Matches(const Input: string; StartPos: Integer): TMatchCollection; overload;
    class function Matches(const Input, Pattern: string): TMatchCollection; overload; static;
    class function Matches(const Input, Pattern: string;
      Options: TRegExOptions): TMatchCollection; overload; static;
    function Replace(const Input, Replacement: string): string; overload;
    function Replace(const Input: string; Evaluator: TMatchEvaluator): string; overload;
    function Replace(const Input, Replacement: string; Count: Integer): string; overload;
    function Replace(const Input: string; Evaluator: TMatchEvaluator;
      Count: Integer): string; overload;
    class function Replace(const Input, Pattern, Replacement: string): string; overload;
      static;
    class function Replace(const Input, Pattern: string;
      Evaluator: TMatchEvaluator): string; overload; static;
    class function Replace(const Input, Pattern, Replacement: string;
      Options: TRegExOptions): string; overload; static;
    class function Replace(const Input, Pattern: string; Evaluator: TMatchEvaluator;
      Options: TRegExOptions): string; overload; static;
    function Split(const Input: string): TArray<string>; overload;
    function Split(const Input: string; Count: Integer): TArray<string>; overload;
    function Split(const Input: string; Count, StartPos: Integer): TArray<string>; overload;
    class function Split(const Input, Pattern: string): TArray<string>; overload; static;
    class function Split(const Input, Pattern: string;
      Options: TRegExOptions): TArray<string>; overload; static;
  end;

implementation

uses
  System.SysUtils;

// The engine's bits of the options: one per ordinal of TRegExOption.
function OptionBits(Options: TRegExOptions): Integer;
var
  O: TRegExOption;
begin
  Result := 0;
  for O in Options do
    Result := Result or (1 shl Ord(O));
end;

// The 0-based start of a search from the 1-based StartPos: up to the end of Input (a
// position before the first is the first).
function CheckStart(const Input: string; StartPos: Integer): Integer;
begin
  if StartPos > Length(Input) then
    raise ERegularExpressionError.Create('start position ' + IntToStr(StartPos) +
      ' is out of the bounds of a string of length ' + IntToStr(Length(Input)));
  if StartPos < 1 then
    Result := 0
  else
    Result := StartPos - 1;
end;

function MakeGroup(const Input: string; const Spans: TArray<Integer>; I: Integer): TGroup;
begin
  Result := Default(TGroup);
  Result.FInput := Input;
  if Spans[I * 2] < 0 then
  begin
    Result.FIndex := 0;
    Result.FLength := 0;
    Result.FSuccess := False;
  end
  else
  begin
    Result.FIndex := Spans[I * 2] + 1;
    Result.FLength := Spans[I * 2 + 1];
    Result.FSuccess := True;
  end;
end;

function MakeMatch(const Input: string; const Spans: TArray<Integer>; Engine: TRegexEngine;
  Subject: TRegexSubject): TMatch;
begin
  Result := Default(TMatch);
  Result.FGroups.FInput := Input;
  Result.FGroups.FSpans := Spans;
  Result.FGroups.FEngine := Engine;
  if Spans = nil then
    Result.FSubject := nil
  else
    Result.FSubject := Subject;
end;

{ TGroup }

function TGroup.GetValue: string;
begin
  if FSuccess then
    Result := Copy(FInput, FIndex, FLength)
  else
    Result := '';
end;

{ TGroupCollectionEnumerator }

function TGroupCollectionEnumerator.MoveNext: Boolean;
begin
  Inc(FIndex);
  Result := FIndex * 2 < Length(FSpans);
end;

function TGroupCollectionEnumerator.GetCurrent: TGroup;
begin
  Result := MakeGroup(FInput, FSpans, FIndex);
end;

{ TGroupCollection }

function TGroupCollection.GetCount: Integer;
begin
  Result := Length(FSpans) div 2;
end;

function TGroupCollection.GetItem(Index: Integer): TGroup;
begin
  if (Index < 0) or (Index >= GetCount) then
    raise ERegularExpressionError.Create('group index ' + IntToStr(Index) +
      ' is out of bounds (' + IntToStr(GetCount) + ' groups)');
  Result := MakeGroup(FInput, FSpans, Index);
end;

function TGroupCollection.GetNamedItem(const Name: string): TGroup;
var
  N: Integer;
begin
  N := -1;
  if FSpans <> nil then
    N := FEngine.GroupNumber(Name);
  if N < 0 then
    raise ERegularExpressionError.Create('there is no group named ''' + Name + '''');
  Result := MakeGroup(FInput, FSpans, N);
end;

function TGroupCollection.GetEnumerator: TGroupCollectionEnumerator;
begin
  Result := Default(TGroupCollectionEnumerator);
  Result.FInput := FInput;
  Result.FSpans := FSpans;
  Result.FIndex := -1;
end;

{ TMatch }

function TMatch.GetIndex: Integer;
begin
  if FGroups.FSpans = nil then
    Result := 0
  else
    Result := FGroups.FSpans[0] + 1;
end;

function TMatch.GetLength: Integer;
begin
  if FGroups.FSpans = nil then
    Result := 0
  else
    Result := FGroups.FSpans[1];
end;

function TMatch.GetSuccess: Boolean;
begin
  Result := FGroups.FSpans <> nil;
end;

function TMatch.GetValue: string;
begin
  if FGroups.FSpans = nil then
    Result := ''
  else
    Result := Copy(FGroups.FInput, FGroups.FSpans[0] + 1, FGroups.FSpans[1]);
end;

function TMatch.NextMatch: TMatch;
var
  E: Integer;
begin
  if FGroups.FSpans = nil then
    Exit(Self);
  // From the end of this match, skipping an empty match there
  E := FGroups.FSpans[0] + FGroups.FSpans[1];
  Result := MakeMatch(FGroups.FInput, FSubject.Find(E, E), FGroups.FEngine, FSubject);
end;

function TMatch.&Result(const Pattern: string): string;
begin
  if FGroups.FSpans = nil then
    Exit('');
  Exit(FSubject.Expand(FGroups.FSpans, Pattern));
end;

{ TMatchCollectionEnumerator }

function TMatchCollectionEnumerator.MoveNext: Boolean;
begin
  Inc(FIndex);
  // A collection never assigned (Default) has no engine and no matches
  Result := (FSpans <> nil) and (FIndex < Length(FSpans) div (FEngine.GroupCount * 2));
end;

function TMatchCollectionEnumerator.GetCurrent: TMatch;
var
  Stride: Integer;
begin
  Stride := FEngine.GroupCount * 2;
  Result := MakeMatch(FInput, Copy(FSpans, FIndex * Stride, Stride), FEngine, FSubject);
end;

{ TMatchCollection }

function TMatchCollection.GetCount: Integer;
begin
  if FSpans = nil then
    Result := 0
  else
    Result := Length(FSpans) div (FEngine.GroupCount * 2);
end;

function TMatchCollection.GetItem(Index: Integer): TMatch;
var
  Stride: Integer;
begin
  if (Index < 0) or (Index >= GetCount) then
    raise ERegularExpressionError.Create('match index ' + IntToStr(Index) +
      ' is out of bounds (' + IntToStr(GetCount) + ' matches)');
  Stride := FEngine.GroupCount * 2;
  Result := MakeMatch(FInput, Copy(FSpans, Index * Stride, Stride), FEngine, FSubject);
end;

function TMatchCollection.GetEnumerator: TMatchCollectionEnumerator;
begin
  Result := Default(TMatchCollectionEnumerator);
  Result.FInput := FInput;
  Result.FSpans := FSpans;
  Result.FEngine := FEngine;
  Result.FSubject := FSubject;
  Result.FIndex := -1;
end;

{ TRegEx }

constructor TRegEx.Create(const Pattern: string; Options: TRegExOptions);
begin
  FEngine := TRegexEngine.Create(Pattern, OptionBits(Options));
end;

function TRegEx.Search(const Input: string; Start, Stop: Integer): TMatch;
var
  Subject: TRegexSubject;
begin
  Subject := FEngine.Subject(Input, Stop);
  Result := MakeMatch(Input, Subject.Find(Start, -1), FEngine, Subject);
end;

function TRegEx.FindAll(const Input: string; Start, Limit: Integer): TMatchCollection;
begin
  Result := Default(TMatchCollection);
  Result.FInput := Input;
  Result.FEngine := FEngine;
  Result.FSubject := FEngine.Subject(Input, Length(Input));
  Result.FSpans := Result.FSubject.FindAll(Start, Limit);
end;

function TRegEx.IsMatch(const Input: string): Boolean;
begin
  Result := FEngine.IsMatch(Input, 0);
end;

function TRegEx.IsMatch(const Input: string; StartPos: Integer): Boolean;
begin
  Result := FEngine.IsMatch(Input, CheckStart(Input, StartPos));
end;

class function TRegEx.IsMatch(const Input, Pattern: string): Boolean;
begin
  Result := TRegEx.Create(Pattern).IsMatch(Input);
end;

class function TRegEx.IsMatch(const Input, Pattern: string; Options: TRegExOptions): Boolean;
begin
  Result := TRegEx.Create(Pattern, Options).IsMatch(Input);
end;

class function TRegEx.Escape(const Str: string; UseWildCards: Boolean): string;
var
  I: Integer;
  C: Char;
begin
  Result := '';
  for I := 1 to Length(Str) do
  begin
    C := Str[I];
    if UseWildCards and (C = '*') then
      Result := Result + '(.*)'
    else if UseWildCards and (C = '?') then
      Result := Result + '(.)'
    else
      case C of
        '\', '.', '*', '+', '?', '(', ')', '[', ']', '{', '}', '|', '^', '$':
          Result := Result + '\' + C;
      else
        Result := Result + C;
      end;
  end;
end;

function TRegEx.Match(const Input: string): TMatch;
begin
  Result := Search(Input, 0, Length(Input));
end;

function TRegEx.Match(const Input: string; StartPos: Integer): TMatch;
begin
  Result := Search(Input, CheckStart(Input, StartPos), Length(Input));
end;

function TRegEx.Match(const Input: string; StartPos, ALength: Integer): TMatch;
var
  Start: Integer;
begin
  Start := CheckStart(Input, StartPos);
  if (ALength < 0) or (ALength > Length(Input) - Start) then
    raise ERegularExpressionError.Create('length ' + IntToStr(ALength) +
      ' from position ' + IntToStr(Start + 1) + ' is out of the bounds of a string of length ' +
      IntToStr(Length(Input)));
  Result := Search(Input, Start, Start + ALength);
end;

class function TRegEx.Match(const Input, Pattern: string): TMatch;
begin
  Result := TRegEx.Create(Pattern).Match(Input);
end;

class function TRegEx.Match(const Input, Pattern: string; Options: TRegExOptions): TMatch;
begin
  Result := TRegEx.Create(Pattern, Options).Match(Input);
end;

function TRegEx.Matches(const Input: string): TMatchCollection;
begin
  Result := FindAll(Input, 0, 0);
end;

function TRegEx.Matches(const Input: string; StartPos: Integer): TMatchCollection;
begin
  Result := FindAll(Input, CheckStart(Input, StartPos), 0);
end;

class function TRegEx.Matches(const Input, Pattern: string): TMatchCollection;
begin
  Result := TRegEx.Create(Pattern).Matches(Input);
end;

class function TRegEx.Matches(const Input, Pattern: string;
  Options: TRegExOptions): TMatchCollection;
begin
  Result := TRegEx.Create(Pattern, Options).Matches(Input);
end;

function TRegEx.Replace(const Input, Replacement: string): string;
begin
  Result := FEngine.Replace(Input, Replacement, 0);
end;

function TRegEx.Replace(const Input: string; Evaluator: TMatchEvaluator): string;
begin
  Result := ReplaceBy(Input, Evaluator, 0);
end;

function TRegEx.Replace(const Input, Replacement: string; Count: Integer): string;
begin
  Result := FEngine.Replace(Input, Replacement, Count);
end;

function TRegEx.Replace(const Input: string; Evaluator: TMatchEvaluator;
  Count: Integer): string;
begin
  Result := ReplaceBy(Input, Evaluator, Count);
end;

function TRegEx.ReplaceBy(const Input: string; const Evaluator: TMatchEvaluator;
  Count: Integer): string;
var
  All: TMatchCollection;
  M: TMatch;
  Last: Integer;
begin
  if Count < 0 then
    Count := 0;
  All := FindAll(Input, 0, Count);
  Result := '';
  // The 1-based position of the first character not yet copied
  Last := 1;
  for M in All do
  begin
    Result := Result + Copy(Input, Last, M.Index - Last) + Evaluator(M);
    Last := M.Index + M.Length;
  end;
  Result := Result + Copy(Input, Last, Length(Input) - Last + 1);
end;

class function TRegEx.Replace(const Input, Pattern, Replacement: string): string;
begin
  Result := TRegEx.Create(Pattern).Replace(Input, Replacement);
end;

class function TRegEx.Replace(const Input, Pattern: string;
  Evaluator: TMatchEvaluator): string;
begin
  Result := TRegEx.Create(Pattern).ReplaceBy(Input, Evaluator, 0);
end;

class function TRegEx.Replace(const Input, Pattern, Replacement: string;
  Options: TRegExOptions): string;
begin
  Result := TRegEx.Create(Pattern, Options).Replace(Input, Replacement);
end;

class function TRegEx.Replace(const Input, Pattern: string; Evaluator: TMatchEvaluator;
  Options: TRegExOptions): string;
begin
  Result := TRegEx.Create(Pattern, Options).ReplaceBy(Input, Evaluator, 0);
end;

function TRegEx.Split(const Input: string): TArray<string>;
begin
  Result := Split(Input, 0, 1);
end;

function TRegEx.Split(const Input: string; Count: Integer): TArray<string>;
begin
  Result := Split(Input, Count, 1);
end;

function TRegEx.Split(const Input: string; Count, StartPos: Integer): TArray<string>;
var
  All: TMatchCollection;
  M: TMatch;
  G: Integer;
  Last: Integer;
  N: Integer;

  procedure Add(const S: string);
  begin
    if N = Length(Result) then
      SetLength(Result, N * 2 + 4);
    Result[N] := S;
    Inc(N);
  end;

begin
  Result := nil;
  if Input = '' then
    Exit;
  N := 0;
  // At most Count parts: Count - 1 matches (all when Count is 0 or less)
  if Count = 1 then
    Add(Input)
  else
  begin
    if Count < 1 then
      Count := 1;
    All := FindAll(Input, CheckStart(Input, StartPos), Count - 1);
    Last := 1;
    for M in All do
    begin
      Add(Copy(Input, Last, M.Index - Last));
      // The groups that took part come between the parts
      for G := 1 to M.Groups.Count - 1 do
        if M.Groups[G].Success then
          Add(M.Groups[G].Value);
      Last := M.Index + M.Length;
    end;
    Add(Copy(Input, Last, Length(Input) - Last + 1));
  end;
  SetLength(Result, N);
end;

class function TRegEx.Split(const Input, Pattern: string): TArray<string>;
begin
  Result := TRegEx.Create(Pattern).Split(Input);
end;

class function TRegEx.Split(const Input, Pattern: string;
  Options: TRegExOptions): TArray<string>;
begin
  Result := TRegEx.Create(Pattern, Options).Split(Input);
end;

end.

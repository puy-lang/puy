// ===== Search (docs/spec/stdlib/System.StrUtils/README.md 2, 3) =====

function StartsStr(const ASubText, AText: string): Boolean;
begin
  Result := AText.StartsWith(ASubText);
end;

function StartsText(const ASubText, AText: string): Boolean;
begin
  Result := AText.StartsWith(ASubText, True);
end;

function EndsStr(const ASubText, AText: string): Boolean;
begin
  Result := AText.EndsWith(ASubText);
end;

function EndsText(const ASubText, AText: string): Boolean;
begin
  Result := AText.EndsWith(ASubText, True);
end;

function ContainsStr(const AText, ASubText: string): Boolean;
begin
  Result := AText.Contains(ASubText);
end;

// AnsiLowerCase maps each character to its lower case when that is one character, which is the
// rule AnsiSameText folds by; it keeps the length, so a match of the folded strings is a match
// of the strings.
function ContainsText(const AText, ASubText: string): Boolean;
begin
  Result := AnsiLowerCase(AText).Contains(AnsiLowerCase(ASubText));
end;

function AnsiStartsStr(const ASubText, AText: string): Boolean;
begin
  Result := StartsStr(ASubText, AText);
end;

function AnsiStartsText(const ASubText, AText: string): Boolean;
begin
  Result := StartsText(ASubText, AText);
end;

function AnsiEndsStr(const ASubText, AText: string): Boolean;
begin
  Result := EndsStr(ASubText, AText);
end;

function AnsiEndsText(const ASubText, AText: string): Boolean;
begin
  Result := EndsText(ASubText, AText);
end;

function AnsiContainsStr(const AText, ASubText: string): Boolean;
begin
  Result := ContainsStr(AText, ASubText);
end;

function AnsiContainsText(const AText, ASubText: string): Boolean;
begin
  Result := ContainsText(AText, ASubText);
end;

function PosEx(const SubStr, S: string; Offset: Integer): Integer;
begin
  if (Offset < 1) or (Offset > Length(S)) then
    Exit(0);
  Result := S.IndexOf(SubStr, Offset - 1) + 1;
end;

function IndexStr(const AText: string; const AValues: array of string): Integer;
var
  I: Integer;
begin
  for I := 0 to High(AValues) do
    if AValues[I] = AText then
      Exit(I);
  Result := -1;
end;

function IndexText(const AText: string; const AValues: array of string): Integer;
var
  I: Integer;
begin
  for I := 0 to High(AValues) do
    if AnsiSameText(AValues[I], AText) then
      Exit(I);
  Result := -1;
end;

function MatchStr(const AText: string; const AValues: array of string): Boolean;
begin
  Result := IndexStr(AText, AValues) >= 0;
end;

function MatchText(const AText: string; const AValues: array of string): Boolean;
begin
  Result := IndexText(AText, AValues) >= 0;
end;

function AnsiIndexStr(const AText: string; const AValues: array of string): Integer;
begin
  Result := IndexStr(AText, AValues);
end;

function AnsiIndexText(const AText: string; const AValues: array of string): Integer;
begin
  Result := IndexText(AText, AValues);
end;

function AnsiMatchStr(const AText: string; const AValues: array of string): Boolean;
begin
  Result := MatchStr(AText, AValues);
end;

function AnsiMatchText(const AText: string; const AValues: array of string): Boolean;
begin
  Result := MatchText(AText, AValues);
end;

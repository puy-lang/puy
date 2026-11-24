// ===== TCharHelper (docs/spec/stdlib/System/Helpers.md) =====

// CharClass (a built-in of the standard library) gives the Unicode properties of a Char as a set
// of bits: 1 Alphabetic, 2 Uppercase, 4 Lowercase, 8 White_Space, 16 a control character (Cc),
// 32 a number (Nd, Nl, No), 64 a decimal digit (Nd).

class function TCharHelper.IsControl(C: Char): Boolean;
begin
  Result := (CharClass(C) and 16) <> 0;
end;

class function TCharHelper.IsDigit(C: Char): Boolean;
begin
  Result := (CharClass(C) and 64) <> 0;
end;

class function TCharHelper.IsHighSurrogate(C: Char): Boolean;
begin
  Result := (C >= #$D800) and (C <= #$DBFF);
end;

class function TCharHelper.IsLetter(C: Char): Boolean;
begin
  Result := (CharClass(C) and 1) <> 0;
end;

class function TCharHelper.IsLetterOrDigit(C: Char): Boolean;
begin
  Result := (CharClass(C) and (1 or 64)) <> 0;
end;

class function TCharHelper.IsLower(C: Char): Boolean;
begin
  Result := (CharClass(C) and 4) <> 0;
end;

class function TCharHelper.IsLowSurrogate(C: Char): Boolean;
begin
  Result := (C >= #$DC00) and (C <= #$DFFF);
end;

class function TCharHelper.IsNumber(C: Char): Boolean;
begin
  Result := (CharClass(C) and 32) <> 0;
end;

class function TCharHelper.IsSurrogate(C: Char): Boolean;
begin
  Result := (C >= #$D800) and (C <= #$DFFF);
end;

class function TCharHelper.IsUpper(C: Char): Boolean;
begin
  Result := (CharClass(C) and 2) <> 0;
end;

class function TCharHelper.IsWhiteSpace(C: Char): Boolean;
begin
  Result := (CharClass(C) and 8) <> 0;
end;

class function TCharHelper.ToLower(C: Char): Char;
begin
  // The mapping of one character to one; a character that maps to more is kept
  Result := System.SysUtils.AnsiLowerCase(C)[1];
end;

class function TCharHelper.ToUpper(C: Char): Char;
begin
  Result := System.SysUtils.AnsiUpperCase(C)[1];
end;

function TCharHelper.IsControl: Boolean;
begin
  Result := (CharClass(Self) and 16) <> 0;
end;

function TCharHelper.IsDigit: Boolean;
begin
  Result := (CharClass(Self) and 64) <> 0;
end;

function TCharHelper.IsHighSurrogate: Boolean;
begin
  Result := (Self >= #$D800) and (Self <= #$DBFF);
end;

function TCharHelper.IsLetter: Boolean;
begin
  Result := (CharClass(Self) and 1) <> 0;
end;

function TCharHelper.IsLetterOrDigit: Boolean;
begin
  Result := (CharClass(Self) and (1 or 64)) <> 0;
end;

function TCharHelper.IsLower: Boolean;
begin
  Result := (CharClass(Self) and 4) <> 0;
end;

function TCharHelper.IsLowSurrogate: Boolean;
begin
  Result := (Self >= #$DC00) and (Self <= #$DFFF);
end;

function TCharHelper.IsNumber: Boolean;
begin
  Result := (CharClass(Self) and 32) <> 0;
end;

function TCharHelper.IsSurrogate: Boolean;
begin
  Result := (Self >= #$D800) and (Self <= #$DFFF);
end;

function TCharHelper.IsUpper: Boolean;
begin
  Result := (CharClass(Self) and 2) <> 0;
end;

function TCharHelper.IsWhiteSpace: Boolean;
begin
  Result := (CharClass(Self) and 8) <> 0;
end;

function TCharHelper.ToLower: Char;
begin
  Result := System.SysUtils.AnsiLowerCase(Self)[1];
end;

function TCharHelper.ToUpper: Char;
begin
  Result := System.SysUtils.AnsiUpperCase(Self)[1];
end;

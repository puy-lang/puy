// ISO 8601 text and UNIX time.

function DateToISO8601(const ADate: TDateTime; AInputIsUTC: Boolean): string;
begin
  Result := FormatDateTime('yyyy-mm-dd"T"hh:nn:ss.zzz', ADate);
  if AInputIsUTC then
    Result := Result + 'Z'
  else
  begin
    var Offset := DuOffsetAtLocal(DuToMillis(ADate)) div SecsPerMin;
    if Offset < 0 then
      Result := Result + '-'
    else
      Result := Result + '+';
    Offset := Abs(Offset);
    Result := Result + DuPad(Offset div MinsPerHour, 2) + ':' + DuPad(Offset mod MinsPerHour, 2);
  end;
end;

// The number written by exactly Count digits of S from Index on; Index moves past them.
function DuDigits(const S: string; var Index: Integer; Count: Integer; out Value: Int64): Boolean;
begin
  Value := 0;
  if Index + Count - 1 > Length(S) then
    Exit(False);
  for var I := Index to Index + Count - 1 do
  begin
    if (S[I] < '0') or (S[I] > '9') then
      Exit(False);
    Value := Value * 10 + (Ord(S[I]) - Ord('0'));
  end;
  Index := Index + Count;
  Result := True;
end;

// Whether S[Index] is C (Index moves past it when it is).
function DuChar(const S: string; var Index: Integer; C: Char): Boolean;
begin
  Result := (Index <= Length(S)) and (S[Index] = C);
  if Result then
    Inc(Index);
end;

function TryISO8601ToDate(const AISODate: string; out Value: TDateTime;
  AReturnUTC: Boolean): Boolean;
var
  Year, Month, Day, Hour, Minute, Second, Fraction, OffsetHours, OffsetMinutes: Int64;
  Date: TDateTime;
begin
  Value := 0;
  var S := AISODate;
  var I := 1;
  if not (DuDigits(S, I, 4, Year) and DuChar(S, I, '-') and DuDigits(S, I, 2, Month) and
    DuChar(S, I, '-') and DuDigits(S, I, 2, Day)) then
    Exit(False);
  if not TryEncodeDate(Year, Month, Day, Date) then
    Exit(False);
  var Millis := Trunc(Date) * Int64(MSecsPerDay);
  if I <= Length(S) then
  begin
    if not (DuChar(S, I, 'T') or DuChar(S, I, 't') or DuChar(S, I, ' ')) then
      Exit(False);
    if not (DuDigits(S, I, 2, Hour) and DuChar(S, I, ':') and DuDigits(S, I, 2, Minute)) then
      Exit(False);
    Second := 0;
    Fraction := 0;
    if DuChar(S, I, ':') then
    begin
      if not DuDigits(S, I, 2, Second) then
        Exit(False);
      if DuChar(S, I, '.') then
      begin
        // The first three digits are the milliseconds; the rest are dropped
        var Digits := 0;
        while (I <= Length(S)) and (S[I] >= '0') and (S[I] <= '9') do
        begin
          if Digits < 3 then
            Fraction := Fraction * 10 + (Ord(S[I]) - Ord('0'));
          Inc(Digits);
          Inc(I);
        end;
        if Digits = 0 then
          Exit(False);
        while Digits < 3 do
        begin
          Fraction := Fraction * 10;
          Inc(Digits);
        end;
      end;
    end;
    if (Hour > 23) or (Minute > 59) or (Second > 59) then
      Exit(False);
    Millis := Millis + ((Hour * 60 + Minute) * 60 + Second) * 1000 + Fraction;
    if I <= Length(S) then
    begin
      if DuChar(S, I, 'Z') or DuChar(S, I, 'z') then
        OffsetMinutes := 0
      else
      begin
        var Sign := 1;
        if DuChar(S, I, '-') then
          Sign := -1
        else if not DuChar(S, I, '+') then
          Exit(False);
        if not DuDigits(S, I, 2, OffsetHours) then
          Exit(False);
        OffsetMinutes := 0;
        if I <= Length(S) then
        begin
          DuChar(S, I, ':');
          if not DuDigits(S, I, 2, OffsetMinutes) then
            Exit(False);
        end;
        if (OffsetHours > 23) or (OffsetMinutes > 59) then
          Exit(False);
        OffsetMinutes := Sign * (OffsetHours * 60 + OffsetMinutes);
      end;
      if I <= Length(S) then
        Exit(False);
      // UTC, then the local time when asked
      Millis := Millis - OffsetMinutes * DuMillisPerMinute;
      if not AReturnUTC then
        Millis := Millis + DuOffsetAtUtc(Millis) * MSecsPerSec;
    end;
  end;
  if (Millis < DuMinMillis) or (Millis >= DuEndMillis) then
    Exit(False);
  Value := DuFromMillis(Millis);
  Result := True;
end;

function ISO8601ToDate(const AISODate: string; AReturnUTC: Boolean): TDateTime;
begin
  if not TryISO8601ToDate(AISODate, Result, AReturnUTC) then
    RaiseConvertError(AISODate, 'ISO 8601 date and time');
end;

function DateTimeToUnix(const AValue: TDateTime; AInputIsUTC: Boolean): Int64;
begin
  var Millis := DuToMillis(AValue);
  if not AInputIsUTC then
    Millis := Millis - DuOffsetAtLocal(Millis) * MSecsPerSec;
  Result := DuFloorDiv(Millis - Int64(UnixDateDelta) * MSecsPerDay, MSecsPerSec);
end;

function UnixToDateTime(const AValue: Int64; AReturnUTC: Boolean): TDateTime;
begin
  // The whole range is below 2^49 milliseconds
  if (AValue > (Int64(1) shl 50) div MSecsPerSec) or
    (AValue < -((Int64(1) shl 50) div MSecsPerSec)) then
    raise ERangeError.Create('the resulting date is out of range (the years 1 to 9999)');
  var Millis := AValue * MSecsPerSec + Int64(UnixDateDelta) * MSecsPerDay;
  if not AReturnUTC then
    Millis := Millis + DuOffsetAtUtc(Millis) * MSecsPerSec;
  Result := DuFromMillis(Millis);
end;

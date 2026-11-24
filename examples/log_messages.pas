program LogMessages;

uses
  System.SysUtils, System.StrUtils, System.Character, System.RegularExpressions;

// Parse log lines and mask account numbers before displaying them.
procedure ShowWarnings(const Log: string);
begin
  var Entry := TRegEx.Create('^(?<level>[A-Z]+)\s+(?<message>.+)$');
  var Account := TRegEx.Create('account=([0-9]+)');
  var Lines := Log.Split([#10]);

  for var Line in Lines do
  begin
    var Match := Entry.Match(Line.Trim);
    if not Match.Success then
      Continue;
    var Level := Match.Groups['level'].Value.ToLower;
    if not MatchText(Level, ['warn', 'error']) then
      Continue;

    var Message := Account.Replace(Match.Groups['message'].Value,
      function(const NumberMatch: TMatch): string
      begin
        var Digits := NumberMatch.Groups[1].Value;
        Result := 'account=' + DupeString('*', Digits.Length);
      end);
    WriteLn(Level, ': ', Message);
  end;
end;

begin
  ShowWarnings('  INFO connected'#10'WARN café account=1234 delayed'#10 +
    'ERROR account=56 retry failed'#10'invalid line');
  var Initial: Char := 'é';
  WriteLn('Unicode letter: ', TCharacter.IsLetter(Initial), ', uppercase: ',
    TCharacter.ToUpper(Initial));
end.

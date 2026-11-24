program RegexReplace;

// TRegEx.Replace, TMatch.Result and TRegEx.Split (spec/stdlib/System.RegularExpressions/README.md
// 5, 6) where Puy and Delphi agree. Delphi XE4 expands the references of a replacement string
// right only for the first match, so the references are tried on inputs with one match.

uses
  System.SysUtils, System.RegularExpressions;

type
  TNumberer = class
    N: Integer;
    function Next(const Match: TMatch): string;
  end;

function TNumberer.Next(const Match: TMatch): string;
begin
  Inc(N);
  Result := '<' + IntToStr(N) + Match.Value + '>';
end;

procedure Show(const A: TArray<string>);
var
  I: Integer;
begin
  Write(Length(A), ':');
  for I := 0 to High(A) do
    Write(' [', A[I], ']');
  WriteLn;
end;

var
  R: TRegEx;
  Numberer: TNumberer;
begin
  R := TRegEx.Create('(\d+)-(?<w>[a-z]+)');
  WriteLn(R.Replace('x 1-a y', '[$1|$2]'));
  WriteLn(R.Replace('x 1-a y', '[\1|\2]'));
  WriteLn(R.Replace('x 1-a y', '[${w}|${1}]'));
  WriteLn(R.Replace('x 1-a y', '[$0|\0|$&]'));
  WriteLn(R.Replace('x 1-a y', '[$$|\\|$|\]'));
  WriteLn(R.Replace('x 1-a y', '[$9|\9|$x|\<w>|\g|${zz}|${]'));
  WriteLn(R.Replace('x 1-a y', '[$12|\12|$01]'));
  WriteLn(R.Replace('x 1-a y', '[$`|$''|$_|$+]'));
  WriteLn(R.Replace('x 1-a y', '[\n\t]'));
  // Without references, every match
  WriteLn(R.Replace('1-a 2-b 3-c', 'X'));
  WriteLn(R.Replace('1-a 2-b 3-c', 'X', 2));
  WriteLn(R.Replace('1-a 2-b 3-c', 'X', 0));
  WriteLn(R.Replace('1-a 2-b 3-c', 'X', -1));
  WriteLn(TRegEx.Replace('aaa', 'a', 'b'));
  WriteLn(TRegEx.Replace('aAa', 'a', 'b', [TRegExOption.roIgnoreCase]));
  WriteLn(TRegEx.Replace('a-b', '(\w)-(\w)', '$2-$1'));
  WriteLn(TRegEx.Replace(#$00E9'b', 'b', '[$`]'));
  WriteLn(TRegEx.Match('x 12-ab', '(\d+)-(?<w>[a-z]+)').Result('[$1|${w}|\2]'));

  // An evaluator
  Numberer := TNumberer.Create;
  R := TRegEx.Create('\d');
  WriteLn(R.Replace('a1b2c3', Numberer.Next));
  WriteLn(R.Replace('a1b2c3', Numberer.Next, 2));
  WriteLn(TRegEx.Replace('a1b2c3', '\d', Numberer.Next));
  WriteLn(TRegEx.Replace('A1b2', '[a-z]', Numberer.Next, [TRegExOption.roIgnoreCase]));
  Numberer.Free;

  // Split
  R := TRegEx.Create(',');
  Show(R.Split('a,b,,c,'));
  Show(R.Split(',a'));
  Show(R.Split(''));
  Show(R.Split('abc'));
  Show(R.Split('a,b,c,d', 2));
  Show(R.Split('a,b,c,d', 0));
  Show(R.Split('a,b,c,d', -1));
  Show(R.Split('a,b,c,d', 1));
  Show(R.Split('a,b,c,d', 0, 3));
  Show(R.Split('x,x,x', 2, 1));
  Show(TRegEx.Split('a1b22c', '\d+'));
  Show(TRegEx.Split('a1b22c', '(\d)+'));
  Show(TRegEx.Split('A1a1A', 'a', [TRegExOption.roIgnoreCase]));
end.

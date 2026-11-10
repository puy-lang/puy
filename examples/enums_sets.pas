program EnumsAndSets;

// Enumerated types, sets and case. Puy requires enumeration values to be
// qualified with their type name (TDay.Mon).

type
  TDay = (Mon, Tue, Wed, Thu, Fri, Sat, Sun);
  TDays = set of TDay;

const
  Weekend: TDays = [TDay.Sat, TDay.Sun];

function Describe(D: TDay): string;
begin
  // Puy needs the else branch here: a case without else never counts as
  // assigning Result on every path, even if its labels cover every value.
  case D of
    TDay.Mon..TDay.Thu: Result := 'workday';
    TDay.Fri: Result := 'almost weekend';
  else
    Result := 'weekend';
  end;
end;

var
  Meetings: TDays;

begin
  for var D := Low(TDay) to High(TDay) do
    WriteLn(D, ': ', Describe(D));

  Meetings := [TDay.Mon, TDay.Wed];
  Include(Meetings, TDay.Fri);
  Exclude(Meetings, TDay.Wed);

  Write('Meetings on:');
  for var D in Meetings do
    Write(' ', D);
  WriteLn;

  if Meetings * Weekend = [] then
    WriteLn('No meetings on the weekend');
  WriteLn('Saturday is free: ', not (TDay.Sat in Meetings));
end.

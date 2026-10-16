use strict;
use warnings;
use Test::More;
use Chilpa::Settings;
use JSON::PP;

my $rendered = Chilpa::Settings::from_html(<<'HTML');
<form>
<input type="hidden" name="challenge" value="private challenge">
<table><tr><td>Account</td><td>Password</td></tr><tr>
<td><input name="username" value="listener"></td>
<td><input type="password" name="password" value="private password"></td>
</tr></table>
<label for="enabled">Enabled</label>
<input id="enabled" name="enabled" type="checkbox" checked value="yes">
<select name="quality"><option value="low">Low</option><option selected value="high">High</option></select>
<input name="connect" type="submit" value="Connect">
<a href="https://example.org/authorize">Authorize</a>
<a href="javascript:alert(1)">Do not run</a>
<a href="java&#10;script:alert(1)">Do not run either</a>
<script>arbitraryCode()</script>
</form>
HTML

my $public = $rendered->{public};
is(scalar @{ $public->{form}{fields} }, 4, 'hidden input is private');
is($public->{form}{fields}[0]{label}, 'Account', 'table labels stay with their column');
is($public->{form}{fields}[1]{label}, 'Password', 'password label is distinct');
is($public->{values}{quality}, 'high', 'selected values survive');
ok(!exists $public->{values}{password}, 'password values never leave the worker');
is(scalar @{ $public->{links} }, 1, 'only safe link schemes survive');
is_deeply($public->{form}{unsupported}, ['scripted settings behavior'], 'scripts remain an explicit limitation');
unlike(encode_json($public), qr/private password|private challenge|arbitraryCode/, 'private data and scripts are absent');
my $params = Chilpa::Settings::submission($rendered, 'connect', { username => 'new', password => 'entered', enabled => JSON::PP::false });
is($params->{challenge}, 'private challenge', 'hidden challenge is restored on the worker');
is($params->{password}, 'entered', 'entered password reaches the action');
is($params->{enabled}, 0, 'unchecked controls reach the backend as false');
eval { Chilpa::Settings::submission($rendered, 'connect', { challenge => 'injected' }) };
like($@, qr/unknown settings field/, 'a client cannot replace a hidden value');
eval { Chilpa::Settings::submission($rendered, 'invented', {}) };
like($@, qr/unknown settings action/, 'unknown actions cannot be invoked');
my $help = Chilpa::Settings::from_html('<fieldset><legend>Account</legend><p>Read <a href="help.html">instructions</a>.</p><input name="username"></fieldset><p role="alert">Connection failed</p>')->{public};
is($help->{form}{fields}[0]{description}, 'Read instructions.', 'field help survives without guest markup');
is($help->{links}[0]{url}, 'help.html', 'help links remain available for host navigation');
is_deeply($help->{notices}, ['Connection failed'], 'authentication failure is visible to the user');
my $document = Chilpa::Settings::from_html('<article><h3>Sign in</h3><p>Open your account settings.</p><script>secretScript()</script></article>')->{public};
is_deeply($document->{content}, [{kind=>'heading',text=>'Sign in'}, {kind=>'paragraph',text=>'Open your account settings.'}], 'help documents become plain structured content');
my $account = Chilpa::Settings::from_html('<table><tr><th>Connected account</th></tr><tr><td>Listener — Studio</td><td><input type="submit" name="remove" value="Remove"></td></tr><tr><td><input type="hidden" name="token" value="private-token"></td></tr></table><p role="alert">Additional authorization failed</p>')->{public};
is_deeply($account->{content}, [{kind=>'paragraph',text=>'Connected account'}, {kind=>'paragraph',text=>'Listener — Studio'}], 'read-only account information survives alongside actions');
is_deeply($account->{notices}, ['Additional authorization failed'], 'a secondary failure does not erase account information');
unlike(encode_json($account), qr/private-token/, 'table extraction never exposes hidden credentials');
done_testing;
